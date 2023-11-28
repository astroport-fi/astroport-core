#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, to_binary, Binary, Deps, Env, Order, StdError, StdResult, Uint128};
use cw_storage_plus::Bound;
use itertools::Itertools;

use astroport::asset::{determine_asset_info, Asset, AssetInfoExt};
use astroport::incentives::{QueryMsg, RewardType, ScheduleResponse, MAX_PAGE_LIMIT};

use crate::error::ContractError;
use crate::state::{
    list_pool_stakers, PoolInfo, UserInfo, BLOCKED_TOKENS, CONFIG, EXTERNAL_REWARD_SCHEDULES,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&CONFIG.load(deps.storage)?)?),
        QueryMsg::Deposit { lp_token, user } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let user_addr = deps.api.addr_validate(&user)?;
            let amount = UserInfo::load_position(deps.storage, &user_addr, &lp_asset)?.amount;
            Ok(to_binary(&amount)?)
        }
        QueryMsg::PendingRewards { lp_token, user } => Ok(to_binary(&query_pending_rewards(
            deps, env, user, lp_token,
        )?)?),
        QueryMsg::RewardInfo { lp_token } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let mut pool_info = PoolInfo::load(deps.storage, &lp_asset)?;
            pool_info.update_rewards(deps.storage, &env, &lp_asset)?;
            Ok(to_binary(&pool_info.rewards)?)
        }
        QueryMsg::BlockedTokensList {} => Ok(to_binary(&BLOCKED_TOKENS.load(deps.storage)?)?),
        QueryMsg::PoolInfo { lp_token } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            Ok(to_binary(
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
            Ok(to_binary(&stakers)?)
        }
        QueryMsg::IsFeeExpected { lp_token, reward } => {
            let lp_asset = determine_asset_info(&lp_token, deps.api)?;
            let pool_info = PoolInfo::may_load(deps.storage, &lp_asset)?;

            let is_fee_expected = pool_info
                .map(|mut x| -> StdResult<_> {
                    // update_rewards() removes finished schedules
                    x.update_rewards(deps.storage, &env, &lp_asset)?;
                    let reward_asset = determine_asset_info(&reward, deps.api)?;

                    let expected = x
                        .rewards
                        .into_iter()
                        .filter(|x| x.reward.is_external())
                        .all(|x| x.reward.asset_info() != &reward_asset);

                    Ok(expected)
                })
                .transpose()?
                .unwrap_or(true);

            Ok(to_binary(&is_fee_expected)?)
        }
        QueryMsg::ExternalRewardSchedules {
            reward,
            lp_token,
            start_after,
            limit,
        } => Ok(to_binary(&query_external_reward_schedules(
            deps,
            env,
            reward,
            lp_token,
            start_after,
            limit,
        )?)?),
    }
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
    let active_rewards = pool_info
        .calculate_rewards(&mut pos)
        .into_iter()
        .map(|(_, asset)| asset);

    let mut outstanding_rewards =
        pos.claim_finished_rewards(deps.storage, &lp_asset, &pool_info)?;
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
        .filter(|x| x.reward.is_external())
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
