#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, Deps, Env, Order, StdError, StdResult, Uint128,
};
use cw_storage_plus::Bound;
use itertools::Itertools;

use astroport::asset::{determine_asset_info, Asset, AssetInfo, AssetInfoExt};
use astroport::incentives::{QueryMsg, RewardType, ScheduleResponse, MAX_PAGE_LIMIT};

use crate::error::ContractError;
use crate::state::{
    list_pool_stakers, PoolInfo, UserInfo, ACTIVE_POOLS, BLOCKED_TOKENS, CONFIG,
    EXTERNAL_REWARD_SCHEDULES, POOLS,
};
use crate::utils::{asset_info_key, from_key_to_asset_info};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?)?),
        QueryMsg::Deposit { lp_token, user } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let user_addr = deps.api.addr_validate(&user)?;
            let amount = UserInfo::may_load_position(deps.storage, &user_addr, &lp_asset)?
                .map(|maybe_pos| maybe_pos.amount)
                .unwrap_or_default();
            Ok(to_json_binary(&amount)?)
        }
        QueryMsg::PendingRewards { lp_token, user } => Ok(to_json_binary(&query_pending_rewards(
            deps, env, user, lp_token,
        )?)?),
        QueryMsg::RewardInfo { lp_token } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let mut pool_info = PoolInfo::load(deps.storage, &lp_asset)?;
            pool_info.update_rewards(deps.storage, &env, &lp_asset)?;
            Ok(to_json_binary(&pool_info.rewards)?)
        }
        QueryMsg::BlockedTokensList { start_after, limit } => Ok(to_json_binary(
            &query_blocked_tokens(deps, start_after, limit)?,
        )?),
        QueryMsg::PoolInfo { lp_token } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            Ok(to_json_binary(
                &PoolInfo::load(deps.storage, &lp_asset)?.into_response(),
            )?)
        }
        QueryMsg::PoolStakers {
            lp_token,
            start_after,
            limit,
        } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let start_after = start_after
                .map(|addr| deps.api.addr_validate(&addr))
                .transpose()?;
            let stakers = list_pool_stakers(deps.storage, &lp_asset, start_after, limit)?;
            Ok(to_json_binary(&stakers)?)
        }
        QueryMsg::IsFeeExpected { lp_token, reward } => {
            let reward_asset = determine_asset_info(&reward, deps.api)?;
            let config = CONFIG.load(deps.storage)?;

            let is_fee_expected = if reward_asset == config.astro_token {
                // ASTRO rewards don't require incentivize fee.
                false
            } else {
                let lp_asset = determine_asset_info(&lp_token, deps.api)?;
                let pool_info = PoolInfo::may_load(deps.storage, &lp_asset)?;

                pool_info
                    .map(|mut x| -> StdResult<_> {
                        // update_rewards() removes finished schedules
                        x.update_rewards(deps.storage, &env, &lp_asset)?;

                        let expected = x
                            .rewards
                            .into_iter()
                            .filter(|x| x.reward.is_external())
                            .all(|x| x.reward.asset_info() != &reward_asset);

                        Ok(expected)
                    })
                    .transpose()?
                    .unwrap_or(true)
            };

            Ok(to_json_binary(&is_fee_expected)?)
        }
        QueryMsg::ExternalRewardSchedules {
            reward,
            lp_token,
            start_after,
            limit,
        } => Ok(to_json_binary(&query_external_reward_schedules(
            deps,
            env,
            reward,
            lp_token,
            start_after,
            limit,
        )?)?),
        QueryMsg::ListPools { start_after, limit } => {
            Ok(to_json_binary(&list_pools(deps, start_after, limit)?)?)
        }
        QueryMsg::ActivePools {} => {
            let pools = ACTIVE_POOLS
                .load(deps.storage)?
                .into_iter()
                .map(|(asset_info, alloc_points)| (asset_info.to_string(), alloc_points))
                .collect_vec();
            Ok(to_json_binary(&pools)?)
        }
    }
}

fn list_pools(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u8>,
) -> StdResult<Vec<String>> {
    let limit = limit.unwrap_or(MAX_PAGE_LIMIT) as usize;
    POOLS
        .keys_raw(
            deps.storage,
            start_after
                .map(|lp_token| determine_asset_info(&lp_token, deps.api))
                .transpose()?
                .as_ref()
                .map(Bound::exclusive),
            None,
            Order::Ascending,
        )
        .map(|item| String::from_utf8(item).map_err(StdError::invalid_utf8))
        .take(limit)
        .collect()
}

fn query_blocked_tokens(
    deps: Deps,
    start_after: Option<AssetInfo>,
    limit: Option<u8>,
) -> StdResult<Vec<AssetInfo>> {
    let limit = limit.unwrap_or(MAX_PAGE_LIMIT) as usize;
    if let Some(start_after) = start_after {
        let asset_key = asset_info_key(&start_after);
        BLOCKED_TOKENS.range(
            deps.storage,
            Some(Bound::exclusive(asset_key.as_slice())),
            None,
            Order::Ascending,
        )
    } else {
        BLOCKED_TOKENS.range(deps.storage, None, None, Order::Ascending)
    }
    .take(limit)
    .map(|item| item.map(|(k, _)| from_key_to_asset_info(k))?)
    .collect()
}

pub fn query_pending_rewards(
    deps: Deps,
    env: Env,
    user: String,
    lp_token: String,
) -> Result<Vec<Asset>, ContractError> {
    let lp_asset = determine_asset_info(&lp_token, deps.api)?;
    let user_addr = deps.api.addr_validate(&user)?;

    let mut pool_info = PoolInfo::load(deps.storage, &lp_asset)?;
    pool_info.update_rewards(deps.storage, &env, &lp_asset)?;

    let mut pos = UserInfo::load_position(deps.storage, &user_addr, &lp_asset)?;

    let mut outstanding_rewards =
        pos.claim_finished_rewards(deps.storage, &lp_asset, &pool_info)?;

    // Reset user reward index for all finished schedules
    pos.reset_user_index(deps.storage, &lp_asset, &pool_info)?;

    let active_rewards = pool_info
        .calculate_rewards(&mut pos)?
        .into_iter()
        .map(|(_, asset)| asset);

    outstanding_rewards.extend(active_rewards);

    let aggregated = outstanding_rewards
        .into_iter()
        .group_by(|asset| asset.info.clone())
        .into_iter()
        .map(|(info, assets)| {
            let amount: Uint128 = assets.into_iter().map(|asset| asset.amount).sum();
            info.with_balance(amount)
        })
        .collect();

    Ok(aggregated)
}

pub fn query_external_reward_schedules(
    deps: Deps,
    env: Env,
    reward: String,
    lp_token: String,
    start_after: Option<u64>,
    limit: Option<u8>,
) -> Result<Vec<ScheduleResponse>, ContractError> {
    let mut limit = limit.unwrap_or(MAX_PAGE_LIMIT).min(MAX_PAGE_LIMIT);
    ensure!(limit > 0, StdError::generic_err("limit must be > 0"));

    let lp_asset = determine_asset_info(&lp_token, deps.api)?;
    let reward_asset = determine_asset_info(&reward, deps.api)?;
    let mut pool_info = PoolInfo::load(deps.storage, &lp_asset)?;
    pool_info.update_rewards(deps.storage, &env, &lp_asset)?;

    let (rps, end_ts) = pool_info
        .rewards
        .iter()
        .find_map(|active| match &active.reward {
            RewardType::Ext {
                info,
                next_update_ts,
            } if info == &reward_asset => Some((active.rps, *next_update_ts)),
            _ => None,
        })
        .ok_or(ContractError::RewardNotFound {
            pool: lp_token,
            reward,
        })?;

    let mut start_after = start_after.unwrap_or_else(|| env.block.time.seconds());
    let mut results = vec![];

    if start_after < end_ts {
        results.push(ScheduleResponse {
            rps,
            start_ts: env.block.time.seconds(),
            end_ts,
        });
        limit -= 1;
        start_after = end_ts
    }
    let from_state = EXTERNAL_REWARD_SCHEDULES
        .prefix((&lp_asset, &reward_asset))
        .range(
            deps.storage,
            Some(Bound::exclusive(start_after)),
            None,
            Order::Ascending,
        )
        .take(limit as usize)
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .map(|(next_update_ts, rps)| {
            let resp = ScheduleResponse {
                rps,
                start_ts: start_after,
                end_ts: next_update_ts,
            };
            start_after = next_update_ts;

            resp
        });

    results.extend(from_state);

    Ok(results)
}
