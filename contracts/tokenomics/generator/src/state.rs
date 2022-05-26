use astroport::asset::{addr_validate_to_lower, AssetInfo};
use astroport::common::OwnershipProposal;
use astroport::DecimalCheckedOps;
use astroport::{
    generator::{PoolInfo, UserInfo},
    generator_proxy::QueryMsg as ProxyQueryMsg,
};
use astroport_governance::voting_escrow::{get_total_voting_power, get_voting_power};

use cosmwasm_std::{Addr, DepsMut, Env, StdResult, Uint128};

use astroport::generator::Config;
use cosmwasm_std::{Decimal, Deps};
use cw20::BalanceResponse;
use cw_storage_plus::{Item, Map};

use std::cmp::min;
use std::collections::HashMap;

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// This is a map that contains information about all generators.
///
/// The first key is the address of a LP token, the second key is an object of type [`PoolInfo`].
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");

/// This is a map that contains information about all stakers.
///
/// The first key is an LP token address, the second key is a depositor address.
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");
/// Previous proxy rewards holder
pub const PROXY_REWARDS_HOLDER: Item<Addr> = Item::new("proxy_rewards_holder");
/// The struct which maps previous proxy addresses to reward assets
pub const PROXY_REWARD_ASSET: Map<&Addr, AssetInfo> = Map::new("proxy_reward_asset");

/// ## Pagination settings
/// The maximum amount of users that can be read at once from [`USER_INFO`]
pub const MAX_LIMIT: u32 = 30;

/// The default amount of users to read from [`USER_INFO`]
pub const DEFAULT_LIMIT: u32 = 10;

/// Contains a proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// The default limit of generators to update user emission
pub const CHECKPOINT_GENERATORS_LIMIT: u32 = 24;

/// Update user balance.
/// ## Params
/// * **user** is an object of type [`UserInfo`].
///
/// * **pool** is an object of type [`PoolInfo`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn update_user_balance(
    mut user: UserInfo,
    pool: &PoolInfo,
    amount: Uint128,
) -> StdResult<UserInfo> {
    user.amount = amount;
    user.reward_user_index = pool.reward_global_index;

    user.reward_debt_proxy = pool
        .accumulated_proxy_rewards_per_share
        .inner_ref()
        .iter()
        .map(|(proxy, rewards_per_share)| {
            let rewards_debt = rewards_per_share.checked_mul(user.amount)?;
            Ok((proxy.clone(), rewards_debt))
        })
        .collect::<StdResult<Vec<_>>>()?
        .into();

    Ok(user)
}

/// ### Description
/// Returns the vector of reward amount per proxy taking into account the amount of debited rewards.
pub fn accumulate_pool_proxy_rewards(
    pool: &PoolInfo,
    user: &UserInfo,
) -> StdResult<Vec<(Addr, Uint128)>> {
    if pool
        .accumulated_proxy_rewards_per_share
        .inner_ref()
        .is_empty()
    {
        return Ok(vec![]);
    }
    let rewards_debt_map: HashMap<_, _> =
        user.reward_debt_proxy.inner_ref().iter().cloned().collect();
    pool.accumulated_proxy_rewards_per_share
        .inner_ref()
        .iter()
        .map(|(proxy, rewards_per_share)| {
            let reward_debt = rewards_debt_map.get(proxy).cloned().unwrap_or_default();
            let pending_proxy_rewards = rewards_per_share
                .checked_mul(user.amount)?
                .saturating_sub(reward_debt);

            Ok((proxy.clone(), pending_proxy_rewards))
        })
        .collect()
}

/// ### Description
/// Saves map between a proxy and an asset info if it is not saved yet.
pub fn update_proxy_asset(deps: DepsMut, proxy_addr: &Addr) -> StdResult<()> {
    if !PROXY_REWARD_ASSET.has(deps.storage, proxy_addr) {
        let proxy_cfg: astroport::generator_proxy::ConfigResponse = deps
            .querier
            .query_wasm_smart(proxy_addr, &astroport::generator_proxy::QueryMsg::Config {})?;
        let asset = AssetInfo::Token {
            contract_addr: addr_validate_to_lower(deps.api, &proxy_cfg.reward_token_addr)?,
        };
        PROXY_REWARD_ASSET.save(deps.storage, proxy_addr, &asset)?
    }

    Ok(())
}

/// ### Description
/// Updates virtual amount for specified user and generator
///
/// **b_u = min(0.4 * b_u + 0.6 * S * (w_i / W), b_u)**
///
/// - b_u is the amount of LP tokens a user staked in a generator
///
/// - S is the total amount of LP tokens staked in a generator
/// - w_i is a user’s current vxASTRO balance
/// - W is the total amount of vxASTRO
pub(crate) fn update_virtual_amount(
    deps: Deps,
    env: &Env,
    cfg: &Config,
    pool: &mut PoolInfo,
    user_info: &mut UserInfo,
    account: &Addr,
    generator: &Addr,
) -> StdResult<()> {
    let mut user_vp = Uint128::zero();
    let mut total_vp = Uint128::zero();

    if let Some(voting_escrow) = &cfg.voting_escrow {
        user_vp = get_voting_power(&deps.querier, voting_escrow, account)?;
        total_vp = get_total_voting_power(&deps.querier, voting_escrow)?;
    }

    let user_virtual_share = user_info.amount.multiply_ratio(4u128, 10u128);

    let lp_balance = if let Some(proxy) = &pool.reward_proxy {
        deps.querier
            .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?
    } else {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            generator,
            &cw20::Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        res.balance
    };
    let total_virtual_share = lp_balance.multiply_ratio(6u8, 10u8);

    let vx_share_emission = if !total_vp.is_zero() {
        Decimal::from_ratio(user_vp, total_vp)
    } else {
        Decimal::zero()
    };

    let current_virtual_amount = min(
        user_virtual_share + vx_share_emission * total_virtual_share,
        user_info.amount,
    );

    pool.total_virtual_supply = pool
        .total_virtual_supply
        .checked_sub(user_info.virtual_amount)?
        .checked_add(current_virtual_amount)?;

    user_info.virtual_amount = current_virtual_amount;

    Ok(())
}
