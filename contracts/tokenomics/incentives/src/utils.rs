use cosmwasm_std::{
    attr, ensure, wasm_execute, Addr, BankMsg, Deps, DepsMut, Env, MessageInfo, Order,
    QuerierWrapper, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128,
};
use itertools::Itertools;

use astroport::asset::{
    determine_asset_info, pair_info_by_pool, AssetInfo, AssetInfoExt, CoinsExt, PairInfo,
};
use astroport::factory::PairType;
use astroport::incentives::{Config, IncentivesSchedule, InputSchedule, MAX_ORPHANED_REWARD_LIMIT};
use astroport::{factory, pair, vesting};

use crate::error::ContractError;
use crate::reply::POST_TRANSFER_REPLY_ID;
use crate::state::{
    Op, PoolInfo, UserInfo, ACTIVE_POOLS, BLOCKED_TOKENS, CONFIG, ORPHANED_REWARDS,
};

/// Claim all rewards and compose [`Response`] object containing all attributes and messages.
/// This function doesn't mutate the state but mutates in-memory objects.
/// Function caller is responsible for updating the state.
/// If vesting_contract is None this function reads config from state and gets vesting address.
pub fn claim_rewards(
    storage: &dyn Storage,
    vesting_contract: Option<Addr>,
    env: Env,
    user: &Addr,
    pool_tuples: Vec<(&AssetInfo, &mut PoolInfo, &mut UserInfo)>,
) -> Result<Response, ContractError> {
    let mut attrs = vec![attr("action", "claim_rewards"), attr("user", user)];
    let mut external_rewards = vec![];
    let mut protocol_reward_amount = Uint128::zero();
    for (lp_token_asset, pool_info, pos) in pool_tuples {
        attrs.push(attr("claimed_position", lp_token_asset.to_string()));

        pool_info.update_rewards(storage, &env, lp_token_asset)?;

        // Claim outstanding rewards from finished schedules
        for finished_reward in pos.claim_finished_rewards(storage, lp_token_asset, pool_info)? {
            if !finished_reward.amount.is_zero() {
                attrs.push(attr("claimed_finished_reward", finished_reward.to_string()));
                external_rewards.push(finished_reward);
            }
        }

        // Reset user reward index for all finished schedules
        pos.reset_user_index(storage, lp_token_asset, pool_info)?;

        for (is_external, reward_asset) in pool_info.calculate_rewards(pos) {
            attrs.push(attr("claimed_reward", reward_asset.to_string()));

            if !reward_asset.amount.is_zero() {
                if is_external {
                    external_rewards.push(reward_asset);
                } else {
                    protocol_reward_amount += reward_asset.amount;
                }
            }
        }

        // Sync user index with pool index. It removes all finished schedules from user info.
        pos.update_and_sync_position(Op::Noop, pool_info);
    }

    // Aggregating rewards by asset info.
    // This allows to reduce number of output messages thus reducing total gas cost.
    let mut messages = external_rewards
        .into_iter()
        .group_by(|asset| asset.info.clone())
        .into_iter()
        .map(|(info, assets)| {
            let amount: Uint128 = assets.into_iter().map(|asset| asset.amount).sum();
            info.with_balance(amount)
                .into_submsg(user, Some((ReplyOn::Error, POST_TRANSFER_REPLY_ID)))
        })
        .collect::<StdResult<Vec<_>>>()?;

    // Claim Astroport rewards
    if !protocol_reward_amount.is_zero() {
        let vesting_contract = if let Some(vesting_contract) = vesting_contract {
            vesting_contract
        } else {
            CONFIG.load(storage)?.vesting_contract
        };
        messages.push(SubMsg::new(wasm_execute(
            vesting_contract,
            &vesting::ExecuteMsg::Claim {
                recipient: Some(user.to_string()),
                amount: Some(protocol_reward_amount),
            },
            vec![],
        )?));
    }

    Ok(Response::new()
        .add_attributes(attrs)
        .add_submessages(messages))
}

/// Only factory can set the allocation points to zero for the specified pool.
/// Called from deregistration context in factory.
pub fn deactivate_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    lp_token: String,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.factory {
        return Err(ContractError::Unauthorized {});
    }

    let lp_token_asset = determine_asset_info(&lp_token, deps.api)?;

    match PoolInfo::may_load(deps.storage, &lp_token_asset)? {
        Some(mut pool_info) if pool_info.is_active_pool() => {
            let mut active_pools = ACTIVE_POOLS.load(deps.storage)?;

            let (ind, _) = active_pools
                .iter()
                .find_position(|(lp_asset, _)| lp_asset == &lp_token_asset)
                .unwrap();
            let (_, alloc_points) = active_pools.swap_remove(ind);

            pool_info.update_rewards(deps.storage, &env, &lp_token_asset)?;
            pool_info.disable_astro_rewards();
            pool_info.save(deps.storage, &lp_token_asset)?;

            config.total_alloc_points = config.total_alloc_points.checked_sub(alloc_points)?;

            for (lp_asset, alloc_points) in &active_pools {
                let mut pool_info = PoolInfo::load(deps.storage, lp_asset)?;
                pool_info.update_rewards(deps.storage, &env, lp_asset)?;
                pool_info.set_astro_rewards(&config, *alloc_points);
                pool_info.save(deps.storage, lp_asset)?;
            }

            ACTIVE_POOLS.save(deps.storage, &active_pools)?;
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new().add_attributes([
                attr("action", "deactivate_pool"),
                attr("lp_token", lp_token),
            ]))
        }
        _ => Ok(Response::new()),
    }
}

/// Removes pools from active pools if their pair type is blocked.
pub fn deactivate_blocked_pools(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let mut active_pools = ACTIVE_POOLS.load(deps.storage)?;
    let mut config = CONFIG.load(deps.storage)?;

    let blocked_pair_types: Vec<PairType> = deps
        .querier
        .query_wasm_smart(&config.factory, &factory::QueryMsg::BlacklistedPairTypes {})?;

    let mut to_remove = vec![];

    for (lp_token_asset, alloc_points) in &active_pools {
        let mut pool_info = PoolInfo::load(deps.storage, lp_token_asset)?;

        let pair_info = query_pair_info(deps.as_ref(), lp_token_asset)?;

        // check if pair type is blocked
        if blocked_pair_types.contains(&pair_info.pair_type) {
            pool_info.update_rewards(deps.storage, &env, lp_token_asset)?;
            pool_info.disable_astro_rewards();
            pool_info.save(deps.storage, lp_token_asset)?;

            config.total_alloc_points = config.total_alloc_points.checked_sub(*alloc_points)?;

            to_remove.push(lp_token_asset.clone());

            response.attributes.extend([
                attr("action", "deactivate_pool"),
                attr("lp_token", lp_token_asset.to_string()),
            ]);
        }
    }

    if !to_remove.is_empty() {
        active_pools.retain(|(lp_token_asset, _)| !to_remove.contains(lp_token_asset));

        for (lp_asset, alloc_points) in &active_pools {
            let mut pool_info = PoolInfo::load(deps.storage, lp_asset)?;
            pool_info.update_rewards(deps.storage, &env, lp_asset)?;
            pool_info.set_astro_rewards(&config, *alloc_points);
            pool_info.save(deps.storage, lp_asset)?;
        }

        ACTIVE_POOLS.save(deps.storage, &active_pools)?;
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(response)
}

pub fn incentivize(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    lp_token: String,
    input: InputSchedule,
) -> Result<Response, ContractError> {
    let schedule = IncentivesSchedule::from_input(&env, &input)?;

    let mut response = Response::new().add_attributes([
        attr("action", "incentivize"),
        attr("lp_token", lp_token.clone()),
        attr("start_ts", env.block.time.seconds().to_string()),
        attr("end_ts", schedule.end_ts.to_string()),
        attr("reward", schedule.reward_info.to_string()),
    ]);

    let lp_token_asset = determine_asset_info(&lp_token, deps.api)?;

    // Prohibit reward schedules with blocked token
    if BLOCKED_TOKENS.has(deps.storage, &asset_info_key(&schedule.reward_info)) {
        return Err(ContractError::BlockedToken {
            token: schedule.reward_info.to_string(),
        });
    }

    let pair_info = query_pair_info(deps.as_ref(), &lp_token_asset)?;
    let config = CONFIG.load(deps.storage)?;
    is_pool_registered(deps.querier, &config, &pair_info, &lp_token)?;

    let mut pool_info = PoolInfo::may_load(deps.storage, &lp_token_asset)?.unwrap_or_default();
    pool_info.update_rewards(deps.storage, &env, &lp_token_asset)?;

    let rewards_number_before = pool_info.rewards.len();
    pool_info.incentivize(deps.storage, &lp_token_asset, &schedule)?;

    let mut funds = info.funds.clone();

    // Check whether this is a new external reward token.
    // 3rd parties are encouraged to keep endless schedules without breaks even with the small rewards.
    // Otherwise, reward token will be removed from the pool info and go to outstanding rewards.
    // Next schedules with the same token will be considered as "new".
    if rewards_number_before < pool_info.rewards.len() {
        // If fee set we expect to receive it
        if let Some(incentivization_fee_info) = &config.incentivization_fee_info {
            let fee_coin_pos = funds
                .iter()
                .find_position(|coin| coin.denom == incentivization_fee_info.fee.denom);
            if let Some((ind, fee_coin)) = fee_coin_pos {
                // Mutate funds array so we can assert below that reward coins properly sent
                funds[ind].amount = fee_coin
                    .amount
                    .checked_sub(incentivization_fee_info.fee.amount)
                    .map_err(|_| ContractError::IncentivizationFeeExpected {
                        fee: incentivization_fee_info.fee.to_string(),
                        lp_token,
                        new_reward_token: schedule.reward_info.to_string(),
                    })?;
                if funds[ind].amount.is_zero() {
                    funds.remove(ind);
                }

                // Send fee to fee receiver
                response = response.add_message(BankMsg::Send {
                    to_address: incentivization_fee_info.fee_receiver.to_string(),
                    amount: vec![incentivization_fee_info.fee.clone()],
                });
            } else {
                return Err(ContractError::IncentivizationFeeExpected {
                    fee: incentivization_fee_info.fee.to_string(),
                    lp_token,
                    new_reward_token: schedule.reward_info.to_string(),
                });
            }
        }
    }

    // Assert that we received reward tokens
    match &schedule.reward_info {
        AssetInfo::Token { contract_addr } => {
            response = response.add_message(wasm_execute(
                contract_addr,
                &cw20::Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: input.reward.amount,
                },
                vec![],
            )?);
        }
        AssetInfo::NativeToken { .. } => {
            funds.assert_coins_properly_sent(&[input.reward], &[schedule.reward_info.clone()])?
        }
    }

    pool_info.save(deps.storage, &lp_token_asset)?;

    Ok(response)
}

pub fn remove_reward_from_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    lp_token: String,
    reward: String,
    bypass_upcoming_schedules: bool,
    receiver: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let lp_asset = determine_asset_info(&lp_token, deps.api)?;
    let reward_asset = determine_asset_info(&reward, deps.api)?;

    let mut pool_info = PoolInfo::load(deps.storage, &lp_asset)?;
    pool_info.update_rewards(deps.storage, &env, &lp_asset)?;
    let unclaimed = pool_info.deregister_reward(
        deps.storage,
        &lp_asset,
        &reward_asset,
        bypass_upcoming_schedules,
    )?;

    pool_info.save(deps.storage, &lp_asset)?;

    let mut response = Response::new();

    // Send unclaimed rewards
    if !unclaimed.is_zero() {
        deps.api.addr_validate(&receiver)?;
        let transfer_msg = reward_asset
            .with_balance(unclaimed)
            .into_submsg(receiver, Some((ReplyOn::Error, POST_TRANSFER_REPLY_ID)))?;
        response = response.add_submessage(transfer_msg);
    }

    Ok(response.add_attributes([
        attr("action", "remove_reward_from_pool"),
        attr("lp_token", lp_token),
        attr("reward", reward),
    ]))
}

/// Queries pair info corresponding to given LP token.
/// Handles both native and cw20 tokens. If the token is native it must follow the following format:
/// factory/{lp_minter}/{token_name} where lp_minter is a valid bech32 address on the current chain.
pub fn query_pair_info(deps: Deps, lp_asset: &AssetInfo) -> StdResult<PairInfo> {
    match lp_asset {
        AssetInfo::Token { contract_addr } => pair_info_by_pool(&deps.querier, contract_addr),
        AssetInfo::NativeToken { denom } => {
            let parts = denom.split('/').collect_vec();
            if denom.starts_with("factory") && parts.len() >= 3 {
                let lp_minter = parts[1];
                deps.api.addr_validate(lp_minter)?;
                deps.querier
                    .query_wasm_smart(lp_minter, &pair::QueryMsg::Pair {})
            } else {
                Err(StdError::generic_err(format!(
                    "LP token {denom} doesn't follow token factory format: factory/{{lp_minter}}/{{token_name}}",
                )))
            }
        }
    }
}

/// Checks if the pool with the following asset infos is registered in the factory contract and
/// LP tokens address/denom matches the one registered in the factory.
pub fn is_pool_registered(
    querier: QuerierWrapper,
    config: &Config,
    pair_info: &PairInfo,
    lp_token_addr: &str,
) -> StdResult<()> {
    querier
        .query_wasm_smart::<PairInfo>(
            &config.factory,
            &factory::QueryMsg::Pair {
                asset_infos: pair_info.asset_infos.to_vec(),
            },
        )
        .map_err(|_| {
            StdError::generic_err(format!(
                "The pair is not registered: {}-{}",
                pair_info.asset_infos[0], pair_info.asset_infos[1]
            ))
        })
        .map(|resp| {
            // Eventually resp.liquidity_token will become just a String once token factory LP tokens are implemented
            if resp.liquidity_token.as_str() == lp_token_addr {
                Ok(())
            } else {
                Err(StdError::generic_err(format!(
                    "LP token {lp_token_addr} doesn't match LP token registered in factory {}",
                    resp.liquidity_token
                )))
            }
        })?
}

pub fn claim_orphaned_rewards(
    deps: DepsMut,
    info: MessageInfo,
    limit: Option<u8>,
    receiver: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let receiver = deps.api.addr_validate(&receiver)?;
    let limit = limit
        .unwrap_or(MAX_ORPHANED_REWARD_LIMIT)
        .min(MAX_ORPHANED_REWARD_LIMIT);

    let orphaned_rewards = ORPHANED_REWARDS
        .range(deps.storage, None, None, Order::Ascending)
        .take(limit as usize)
        .collect::<StdResult<Vec<_>>>()?;

    if orphaned_rewards.is_empty() {
        return Err(ContractError::NoOrphanedRewards {});
    }

    let mut messages = vec![];
    let mut attrs = vec![
        attr("action", "claim_orphaned_rewards"),
        attr("receiver", &receiver),
    ];

    for (reward_info_binary, amount) in orphaned_rewards {
        // Send orphaned rewards
        if !amount.is_zero() {
            ORPHANED_REWARDS.remove(deps.storage, &reward_info_binary);

            let reward_info = from_key_to_asset_info(reward_info_binary)?;
            let reward_asset = reward_info.with_balance(amount);

            attrs.push(attr("claimed_orphaned_reward", reward_asset.to_string()));

            let transfer_msg = reward_asset
                .into_submsg(&receiver, Some((ReplyOn::Error, POST_TRANSFER_REPLY_ID)))?;
            messages.push(transfer_msg);
        }
    }

    Ok(Response::new().add_submessages(messages))
}

pub fn asset_info_key(asset_info: &AssetInfo) -> Vec<u8> {
    let mut bytes = vec![];
    match asset_info {
        AssetInfo::NativeToken { denom } => {
            bytes.push(0);
            bytes.extend_from_slice(denom.as_bytes());
        }
        AssetInfo::Token { contract_addr } => {
            bytes.push(1);
            bytes.extend_from_slice(contract_addr.as_bytes());
        }
    }

    bytes
}

pub fn from_key_to_asset_info(bytes: Vec<u8>) -> StdResult<AssetInfo> {
    match bytes[0] {
        0 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::native),
        1 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::cw20_unchecked),
        _ => Err(StdError::generic_err(
            "Failed to deserialize asset info key",
        )),
    }
}

#[cfg(test)]
mod unit_tests {
    use astroport::asset::AssetInfo;

    use super::*;

    #[test]
    fn test_asset_info_binary_key() {
        let asset_infos = vec![
            AssetInfo::native("uusd"),
            AssetInfo::cw20_unchecked("wasm1contractxxx"),
        ];

        for asset_info in asset_infos {
            let key = asset_info_key(&asset_info);
            assert_eq!(from_key_to_asset_info(key).unwrap(), asset_info);
        }
    }

    #[test]
    fn test_deserialize_asset_info_from_malformed_data() {
        let asset_infos = vec![
            AssetInfo::native("uusd"),
            AssetInfo::cw20_unchecked("wasm1contractxxx"),
        ];

        for asset_info in asset_infos {
            let mut key = asset_info_key(&asset_info);
            key[0] = 2;

            assert_eq!(
                from_key_to_asset_info(key).unwrap_err(),
                StdError::generic_err("Failed to deserialize asset info key")
            );
        }

        let key = vec![0, u8::MAX];
        assert_eq!(
            from_key_to_asset_info(key).unwrap_err().to_string(),
            "Cannot decode UTF8 bytes into string: invalid utf-8 sequence of 1 bytes from index 0"
        );
    }
}
