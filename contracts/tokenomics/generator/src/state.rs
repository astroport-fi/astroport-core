use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::generator::{PoolInfo, PoolInfoV2};
use astroport::DecimalCheckedOps;
use cosmwasm_std::{Addr, StdError, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// This structure stores the outstanding amount of token rewards that a user accrued.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    /// The amount of LP tokens staked
    pub amount: Uint128,
    /// The amount of ASTRO rewards a user already received or is not eligible for; used for proper reward calculation
    pub reward_debt: Uint128,
    /// Proxy reward amount a user already received or is not eligible for; used for proper reward calculation
    pub reward_debt_proxy: Uint128,
}

/// This structure stores the outstanding amount of token rewards that a user accrued.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfoV2 {
    /// The amount of LP tokens staked
    pub amount: Uint128,
    /// The amount of ASTRO rewards a user already received or is not eligible for; used for proper reward calculation
    pub reward_debt: Uint128,
    /// Proxy reward amount a user already received per reward proxy; used for proper reward calculation
    /// Vector of pairs (reward_proxy, reward debited).
    pub reward_debt_proxy: Vec<(Addr, Uint128)>,
}

/// This structure stores the core parameters for the Generator contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_point: Uint128,
    /// The block number when the ASTRO distribution starts
    pub start_block: Uint64,
    /// The list of allowed proxy reward contracts
    pub allowed_reward_proxies: Vec<Addr>,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
    /// The list of active pools with allocation points
    pub active_pools: Vec<(Addr, Uint128)>,
    /// The blocked list of tokens
    pub blocked_list_tokens: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteOnReply {
    /// Updates reward and returns it to user.
    ClaimRewards {
        /// The list of LP tokens contract
        lp_tokens: Vec<Addr>,
        /// The rewards recipient
        account: Addr,
    },
    /// Stake LP tokens in the Generator to receive token emissions
    Deposit {
        /// The LP token to stake
        lp_token: Addr,
        /// The account that receives ownership of the staked tokens
        account: Addr,
        /// The amount of tokens to deposit
        amount: Uint128,
    },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The LP tokens to withdraw
        lp_token: Addr,
        /// The account that receives the withdrawn LP tokens
        account: Addr,
        /// The amount of tokens to withdraw
        amount: Uint128,
    },
    /// Sets a new amount of ASTRO to distribute per block between all active generators
    SetTokensPerBlock {
        /// The new amount of ASTRO to distribute per block
        amount: Uint128,
    },
    /// Migrate LP tokens and collected rewards to new proxy
    MigrateProxy { lp_addr: Addr, new_proxy_addr: Addr },
    /// Stake LP tokens into new reward proxy
    MigrateProxyDepositLP {
        lp_addr: Addr,
        new_proxy_addr: Addr,
        amount: Uint128,
    },
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// This is a map that contains information about all generators.
///
/// The first key is the address of a LP token, the second key is an object of type [`PoolInfo`].
pub const POOL_INFO: Map<&Addr, PoolInfoV2> = Map::new("pool_info");
/// Old POOL_INFO storage interface for backward compatibility
pub const OLD_POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");

pub trait CompatibleLoader<K, R> {
    fn compatible_load(&self, store: &dyn Storage, key: K) -> StdResult<R>;
}

impl CompatibleLoader<&Addr, PoolInfoV2> for Map<'_, &Addr, PoolInfoV2> {
    fn compatible_load(&self, store: &dyn Storage, key: &Addr) -> StdResult<PoolInfoV2> {
        match self.load(store, key) {
            Ok(pool_info) => Ok(pool_info),
            Err(_) => {
                let pool_info = OLD_POOL_INFO.load(store, key)?;
                let mut accumulated_proxy_rewards_per_share = vec![];
                let mut orphan_proxy_rewards = vec![];
                if let Some(proxy) = pool_info.reward_proxy.clone() {
                    if !pool_info.accumulated_proxy_rewards_per_share.is_zero() {
                        accumulated_proxy_rewards_per_share =
                            vec![(proxy.clone(), pool_info.accumulated_proxy_rewards_per_share)];
                        orphan_proxy_rewards = vec![(proxy, pool_info.orphan_proxy_rewards)]
                    }
                }
                let pool_info_v2 = PoolInfoV2 {
                    last_reward_block: pool_info.last_reward_block,
                    accumulated_rewards_per_share: pool_info.accumulated_rewards_per_share,
                    reward_proxy: pool_info.reward_proxy,
                    accumulated_proxy_rewards_per_share,
                    proxy_reward_balance_before_update: pool_info
                        .proxy_reward_balance_before_update,
                    orphan_proxy_rewards,
                    has_asset_rewards: pool_info.has_asset_rewards,
                };
                Ok(pool_info_v2)
            }
        }
    }
}

pub const TMP_USER_ACTION: Item<Option<ExecuteOnReply>> = Item::new("tmp_user_action");

/// This is a map that contains information about all stakers.
///
/// The first key is an LP token address, the second key is a depositor address.
pub const USER_INFO: Map<(&Addr, &Addr), UserInfoV2> = Map::new("user_info");
/// Old USER_INFO storage interface for backward compatibility
pub const OLD_USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");

impl CompatibleLoader<(&Addr, &Addr), UserInfoV2> for Map<'_, (&Addr, &Addr), UserInfoV2> {
    fn compatible_load(&self, store: &dyn Storage, key: (&Addr, &Addr)) -> StdResult<UserInfoV2> {
        match self.load(store, key) {
            Ok(user_info) => Ok(user_info),
            Err(_) => {
                let user_info = OLD_USER_INFO.load(store, key)?;
                let pool_info = POOL_INFO.compatible_load(store, key.0)?;
                let mut reward_debt_proxy = vec![];
                if let Some(reward_proxy) = pool_info.reward_proxy {
                    reward_debt_proxy = vec![(reward_proxy, user_info.reward_debt_proxy)]
                }

                let user_info = UserInfoV2 {
                    amount: user_info.amount,
                    reward_debt: user_info.reward_debt,
                    reward_debt_proxy,
                };

                Ok(user_info)
            }
        }
    }
}

/// ## Pagination settings
/// The maximum amount of users that can be read at once from [`USER_INFO`]
pub const MAX_LIMIT: u32 = 30;

/// The default amount of users to read from [`USER_INFO`]
pub const DEFAULT_LIMIT: u32 = 10;

/// Contains a proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Update user balance.
/// ## Params
/// * **user** is an object of type [`UserInfo`].
///
/// * **pool** is an object of type [`PoolInfo`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn update_user_balance(
    mut user: UserInfoV2,
    pool: &PoolInfoV2,
    amount: Uint128,
) -> StdResult<UserInfoV2> {
    user.amount = amount;

    if !pool.accumulated_rewards_per_share.is_zero() {
        user.reward_debt = pool
            .accumulated_rewards_per_share
            .checked_mul(user.amount)?;
    };

    Ok(user)
}

pub fn accumulate_pool_proxy_rewards(
    pool: &PoolInfoV2,
    user: &mut UserInfoV2,
) -> StdResult<Vec<(Addr, Uint128)>> {
    if !pool.accumulated_proxy_rewards_per_share.is_empty() {
        let rewards_debt_map: HashMap<_, _> = user.reward_debt_proxy.iter().cloned().collect();
        user.reward_debt_proxy = vec![];
        pool.accumulated_proxy_rewards_per_share
            .iter()
            .filter(|(_, rewards_per_share)| !rewards_per_share.is_zero())
            .map(|(proxy, rewards_per_share)| {
                let reward_debt = *rewards_debt_map.get(&proxy).ok_or_else(|| {
                    StdError::generic_err(format!("Inconsistent proxy ({}) rewards state.", proxy))
                })?;
                let pending_proxy_rewards = rewards_per_share
                    .checked_mul(user.amount)?
                    .saturating_sub(reward_debt);
                // Change the user's reward debt to the new value.
                user.reward_debt_proxy
                    .push((proxy.clone(), pending_proxy_rewards));

                Ok((proxy.clone(), pending_proxy_rewards))
            })
            .collect()
    } else {
        Ok(vec![])
    }
}
