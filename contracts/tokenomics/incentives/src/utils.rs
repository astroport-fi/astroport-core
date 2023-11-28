use cosmwasm_std::{
    attr, wasm_execute, Addr, Deps, DepsMut, Env, MessageInfo, QuerierWrapper, Response, StdError,
    StdResult, Storage, Uint128,
};
use itertools::Itertools;

use astroport::asset::{
    determine_asset_info, pair_info_by_pool, AssetInfo, AssetInfoExt, CoinsExt, PairInfo,
};
use astroport::factory::PairType;
use astroport::{factory, vesting};

use crate::error::ContractError;
use crate::state::{Op, PoolInfo, UserInfo, ACTIVE_POOLS, BLOCKED_TOKENS, CONFIG};
use astroport::incentives::{Config, IncentivesSchedule, InputSchedule};

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

        // Claim outstanding rewards from finished schedules
        for finished_reward in pos.claim_finished_rewards(storage, lp_token_asset, pool_info)? {
            if !finished_reward.amount.is_zero() {
                attrs.push(attr("claimed_finished_reward", finished_reward.to_string()));
                external_rewards.push(finished_reward);
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
            info.with_balance(amount).into_msg(user)
        })
        .collect::<StdResult<Vec<_>>>()?;

    // Claim Astroport rewards
    if !protocol_reward_amount.is_zero() {
        let vesting_contract = if let Some(vesting_contract) = vesting_contract {
            vesting_contract
        } else {
            CONFIG.load(storage)?.vesting_contract
        };
        messages.push(
            wasm_execute(
                vesting_contract,
                &vesting::ExecuteMsg::Claim {
                    recipient: Some(user.to_string()),
                    amount: Some(protocol_reward_amount),
                },
                vec![],
            )?
            .into(),
        );
    }

    Ok(Response::new().add_attributes(attrs).add_messages(messages))
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
    let mut pool_info = PoolInfo::load(deps.storage, &lp_token_asset)?;

    let mut response = Response::new();

    if pool_info.is_active_pool() {
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

        response.attributes.extend([
            attr("action", "deactivate_pool"),
            attr("lp_token", lp_token),
        ]);
    }

    Ok(response)
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
    let blocked_tokens = BLOCKED_TOKENS.load(deps.storage)?;
    if blocked_tokens.contains(&schedule.reward_info) {
        return Err(ContractError::BlockedToken {
            token: schedule.reward_info.to_string(),
        });
    }

    let pair_info = query_pair_info(deps.as_ref(), &lp_token_asset)?;
    let config = CONFIG.load(deps.storage)?;
    is_pool_registered(deps.querier, &config, &pair_info)?;

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
        let transfer_msg = reward_asset.with_balance(unclaimed).into_msg(receiver)?;
        response = response.add_message(transfer_msg);
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
                let lp_minter = denom.split('/').nth(1).unwrap();
                deps.api.addr_validate(lp_minter)?;
                pair_info_by_pool(&deps.querier, lp_minter)
            } else {
                Err(StdError::generic_err(format!(
                    "LP token {denom} doesn't follow token factory format: factory/{{lp_minter}}/{{token_name}}",
                )))
            }
        }
    }
}

/// Checks if the pool with the following asset infos is registered in the factory contract.
pub fn is_pool_registered(
    querier: QuerierWrapper,
    config: &Config,
    pair_info: &PairInfo,
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
            if resp.contract_addr == pair_info.contract_addr {
                Ok(())
            } else {
                Err(StdError::generic_err(format!(
                    "LP token minter (pair) {} doesn't match pair address registered in factory {}",
                    pair_info.contract_addr, resp.contract_addr
                )))
            }
        })?
}
