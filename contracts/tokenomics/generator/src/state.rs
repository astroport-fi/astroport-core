use astroport::common::OwnershipProposal;
use astroport::DecimalCheckedOps;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main information of each user
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    /// Sets an amount
    pub amount: Uint128,
    /// Sets a reward debt TODO:
    pub reward_debt: Uint128,
    /// Sets a reward debtor proxy
    pub reward_debt_proxy: Uint128,
}

/// ## Description
/// This structure describes the main information of pool
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    /// Sets the allocation point
    pub alloc_point: Uint64,
    /// Sets the last reward block
    pub last_reward_block: Uint64,
    pub accumulated_rewards_per_share: Decimal,
    /// Sets the reward proxy contract
    pub reward_proxy: Option<Addr>,
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// Sets the orphan proxy rewards which are left by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
}

/// ## Description
/// This structure describes the main control config of generator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Sets contract address that used for controls settings
    pub owner: Addr,
    /// Sets the ASTRO token address
    pub astro_token: Addr,
    /// Sets the ASTRO tokens created per block.
    pub tokens_per_block: Uint128,
    /// Sets the total allocation points. Must be the sum of all allocation points in all pools.
    pub total_alloc_point: Uint64,
    /// Sets the block number when ASTRO mining starts.
    pub start_block: Uint64,
    /// Sets the list of allowed reward proxy contracts
    pub allowed_reward_proxies: Vec<Addr>,
    /// Sets the vesting contract from which rewards are received
    pub vesting_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteOnReply {
    /// Updates reward for all pools
    MassUpdatePools {},
    /// Add a new pool with allocation point
    Add {
        /// Sets the LP token contract
        lp_token: Addr,
        /// Sets the allocation point for LP token contract
        alloc_point: Uint64,
        /// Sets the reward proxy contract
        reward_proxy: Option<String>,
    },
    Set {
        lp_token: Addr,
        alloc_point: Uint64,
    },
    UpdatePool {
        lp_token: Addr,
    },
    Deposit {
        lp_token: Addr,
        account: Addr,
        amount: Uint128,
    },
    Withdraw {
        lp_token: Addr,
        account: Addr,
        amount: Uint128,
    },
    SetTokensPerBlock {
        amount: Uint128,
    },
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// ## Description
/// This is a map that contains information about all liquidity pools.
///
/// The first key part is liquidity pool token, the second key part is an object of type [`PoolInfo`].
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");
pub const TMP_USER_ACTION: Item<Option<ExecuteOnReply>> = Item::new("tmp_user_action");

/// ## Description
/// This is a map that contains information about all users.
///
/// The first key part is token, the second key part is depositor.
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub fn get_pools(store: &dyn Storage) -> Vec<(Addr, PoolInfo)> {
    POOL_INFO
        .range(store, None, None, cosmwasm_std::Order::Ascending)
        .filter_map(|v| {
            v.ok()
                .map(|v| (Addr::unchecked(String::from_utf8(v.0).unwrap()), v.1))
        })
        .collect()
}

pub fn update_user_balance(
    mut user: UserInfo,
    pool: &PoolInfo,
    amount: Uint128,
) -> StdResult<UserInfo> {
    user.amount = amount;

    if !pool.accumulated_rewards_per_share.is_zero() {
        user.reward_debt = pool
            .accumulated_rewards_per_share
            .checked_mul(user.amount)?;
    };

    if !pool.accumulated_proxy_rewards_per_share.is_zero() {
        user.reward_debt_proxy = pool
            .accumulated_proxy_rewards_per_share
            .checked_mul(user.amount)?;
    };

    Ok(user)
}
