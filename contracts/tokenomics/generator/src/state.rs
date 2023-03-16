use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::restricted_vector::RestrictedVector;
use astroport::DecimalCheckedOps;
use astroport::{
    generator::{PoolInfo, UserInfo, UserInfoV2},
    generator_proxy::QueryMsg as ProxyQueryMsg,
};
use astroport_governance::voting_escrow::{get_total_voting_power, get_voting_power};
use astroport_governance::voting_escrow_delegation::get_adjusted_balance;
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, QuerierWrapper, StdResult, Storage, Uint128};

use astroport::generator::Config;
use cw20::BalanceResponse;
use cw_storage_plus::{Item, Map};

use std::collections::HashMap;

/// Constants to update user's virtual amount. For more info see update_virtual_amount() documentation.
/// 0.4 of the LP tokens amount.
const REAL_SHARE: Decimal = Decimal::raw(400000000000000000);
/// 0.6 of the user's voting power aka vxASTRO balance.
const VXASTRO_SHARE: Decimal = Decimal::raw(600000000000000000);

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// This is a map that contains information about all generators.
///
/// The first key is the address of a LP token, the second key is an object of type [`PoolInfo`].
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");

/// This is a map that contains information about all stakers.
///
/// The first key is an LP token address, the second key is a depositor address.
pub const USER_INFO: Map<(&Addr, &Addr), UserInfoV2> = Map::new("user_info");
/// Old USER_INFO storage interface for backward compatibility
pub const OLD_USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");
/// Previous proxy rewards holder
pub const PROXY_REWARDS_HOLDER: Item<Addr> = Item::new("proxy_rewards_holder");
/// The struct which maps previous proxy addresses to reward assets
pub const PROXY_REWARD_ASSET: Map<&Addr, AssetInfo> = Map::new("proxy_reward_asset");

pub trait CompatibleLoader<K, R> {
    fn compatible_load(&self, store: &dyn Storage, key: K) -> StdResult<R>;
}

impl CompatibleLoader<(&Addr, &Addr), UserInfoV2> for Map<'_, (&Addr, &Addr), UserInfoV2> {
    fn compatible_load(&self, store: &dyn Storage, key: (&Addr, &Addr)) -> StdResult<UserInfoV2> {
        self.load(store, key).or_else(|_| {
            let old_user_info = OLD_USER_INFO.load(store, key)?;
            let pool_info = POOL_INFO.load(store, key.0)?;
            let mut reward_debt_proxy = RestrictedVector::default();
            if let Some((first_reward_proxy, _)) = pool_info
                .accumulated_proxy_rewards_per_share
                .inner_ref()
                .first()
            {
                reward_debt_proxy = RestrictedVector::new(
                    first_reward_proxy.clone(),
                    old_user_info.reward_debt_proxy,
                )
            }

            let current_reward = pool_info
                .reward_global_index
                .checked_mul_uint128(old_user_info.amount)?
                .checked_sub(old_user_info.reward_debt)?;

            let user_index = pool_info.reward_global_index
                - Decimal::from_ratio(current_reward, old_user_info.amount);

            let user_info = UserInfoV2 {
                amount: old_user_info.amount,
                reward_user_index: user_index,
                reward_debt_proxy,
                virtual_amount: old_user_info.amount,
            };

            Ok(user_info)
        })
    }
}

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
pub fn update_user_balance(
    mut user: UserInfoV2,
    pool: &PoolInfo,
    amount: Uint128,
) -> StdResult<UserInfoV2> {
    user.amount = amount;
    user.reward_user_index = pool.reward_global_index;

    user.reward_debt_proxy = pool
        .accumulated_proxy_rewards_per_share
        .inner_ref()
        .iter()
        .map(|(proxy, rewards_per_share)| {
            let rewards_debt = rewards_per_share.checked_mul_uint128(user.amount)?;
            Ok((proxy.clone(), rewards_debt))
        })
        .collect::<StdResult<Vec<_>>>()?
        .into();

    Ok(user)
}

/// Returns the vector of reward amount per proxy taking into account the amount of debited rewards.
pub fn accumulate_pool_proxy_rewards(
    pool: &PoolInfo,
    user: &UserInfoV2,
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
                .checked_mul_uint128(user.amount)?
                .saturating_sub(reward_debt);

            Ok((proxy.clone(), pending_proxy_rewards))
        })
        .collect()
}

/// Saves map between a proxy and an asset info if it is not saved yet.
pub fn update_proxy_asset(deps: DepsMut, proxy_addr: &Addr) -> StdResult<()> {
    if !PROXY_REWARD_ASSET.has(deps.storage, proxy_addr) {
        let proxy_cfg: astroport::generator_proxy::ConfigResponse = deps
            .querier
            .query_wasm_smart(proxy_addr, &astroport::generator_proxy::QueryMsg::Config {})?;
        let asset = AssetInfo::Token {
            contract_addr: deps.api.addr_validate(&proxy_cfg.reward_token_addr)?,
        };
        PROXY_REWARD_ASSET.save(deps.storage, proxy_addr, &asset)?
    }

    Ok(())
}

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
    querier: QuerierWrapper,
    cfg: &Config,
    pool: &mut PoolInfo,
    user_info: &mut UserInfoV2,
    account: &Addr,
    lp_balance: Uint128,
) -> StdResult<()> {
    let mut user_vp = Uint128::zero();
    let mut total_vp = Uint128::zero();

    if let Some(voting_escrow) = &cfg.voting_escrow {
        if let Some(voting_delegation) = &cfg.voting_escrow_delegation {
            user_vp = get_adjusted_balance(
                &querier,
                voting_delegation.to_string(),
                account.to_string(),
                None,
            )?;
        } else {
            user_vp = get_voting_power(&querier, voting_escrow, account.to_string())?;
        }

        total_vp = get_total_voting_power(&querier, voting_escrow)?;
    }

    let user_virtual_share = user_info.amount * REAL_SHARE;

    let total_virtual_share = lp_balance * VXASTRO_SHARE;

    let vx_share_emission = if !total_vp.is_zero() {
        Decimal::from_ratio(user_vp, total_vp)
    } else {
        Decimal::zero()
    };

    let current_virtual_amount = user_info
        .amount
        .min(user_virtual_share + vx_share_emission * total_virtual_share);

    pool.total_virtual_supply = pool
        .total_virtual_supply
        .checked_sub(user_info.virtual_amount)?
        .checked_add(current_virtual_amount)?;

    user_info.virtual_amount = current_virtual_amount;

    Ok(())
}

/// Query total LP tokens balance for specified generator.
/// If tokens are staked in proxy, then query proxy balance. Otherwise query generator contract balance.
pub(crate) fn query_lp_balance(
    deps: Deps,
    generator_addr: &Addr,
    lp_token: &Addr,
    pool_info: &PoolInfo,
) -> StdResult<Uint128> {
    let lp_amount = if let Some(proxy) = &pool_info.reward_proxy {
        deps.querier
            .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?
    } else {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            lp_token,
            &cw20::Cw20QueryMsg::Balance {
                address: generator_addr.to_string(),
            },
        )?;
        res.balance
    };
    Ok(lp_amount)
}
