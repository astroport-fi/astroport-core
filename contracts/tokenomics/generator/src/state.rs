use astroport::common::OwnershipProposal;
use astroport::DecimalCheckedOps;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
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

/// ## Description
/// This structure stores information about a specific generator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    /// This is the share of ASTRO rewards that this generator receives every block
    pub alloc_point: Uint64,
    /// Accumulated amount of rewards per LP unit. Used for reward calculations
    pub last_reward_block: Uint64,
    /// This is the accrued amount of rewards up to the latest checkpoint
    pub accumulated_rewards_per_share: Decimal,
    /// The 3rd party proxy reward contract
    pub reward_proxy: Option<Addr>,
    /// This is the accrued amount of 3rd party rewards up to the latest checkpoint
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// This is the balance of 3rd party proxy rewards that the proxy had before a reward snapshot
    pub proxy_reward_balance_before_update: Uint128,
    /// The orphaned proxy rewards which are left behind by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
    /// Whether a generator receives 3rd party rewards or not
    pub has_asset_rewards: bool,
}

/// ## Description
/// This structure stores the core parameters for the Generator contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_point: Uint64,
    /// The block number when the ASTRO distribution starts
    pub start_block: Uint64,
    /// The list of allowed proxy reward contracts
    pub allowed_reward_proxies: Vec<Addr>,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteOnReply {
    /// Updates the amount of already distributed rewards for multiple active generators
    MassUpdatePools {},
    /// Create a new generator
    Add {
        /// The LP token contract that can be staked in the generator
        lp_token: Addr,
        /// The slice of ASTRO emissions received by this generator every block
        alloc_point: Uint64,
        /// This flag indicates whether the generator has 3rd party rewards or not
        has_asset_rewards: bool,
        /// The dual rewards proxy contract for this generator
        reward_proxy: Option<String>,
    },
    /// Update a pool's ASTRO allocation points
    Set {
        /// The LP token contract for which we change emissions
        lp_token: Addr,
        /// The new slice of ASTRO emissions received by `lp_token` stakers every block
        alloc_point: Uint64,
        /// This flag indicates whether the generator has 3rd party rewards or not
        has_asset_rewards: bool,
    },
    /// Updates the amount of already distributed rewards for a specific generator
    UpdatePool {
        /// The LP token whose generator we update
        lp_token: Addr,
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
}

/// ## Description
/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// ## Description
/// This is a map that contains information about all generators.
///
/// The first key is the address of a LP token, the second key is an object of type [`PoolInfo`].
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");
pub const TMP_USER_ACTION: Item<Option<ExecuteOnReply>> = Item::new("tmp_user_action");

/// ## Description
/// This is a map that contains information about all stakers.
///
/// The first key is an LP token address, the second key is a depositor address.
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");

/// ## Pagination settings
/// The maximum amount of users that can be read at once from [`USER_INFO`]
pub const MAX_LIMIT: u32 = 30;

/// The default amount of users to read from [`USER_INFO`]
pub const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Contains a proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// ## Description
/// This function returns the list of instantiated generators.
pub fn get_pools(store: &dyn Storage) -> Vec<(Addr, PoolInfo)> {
    POOL_INFO
        .range(store, None, None, cosmwasm_std::Order::Ascending)
        .filter_map(|v| {
            v.ok()
                .map(|v| (Addr::unchecked(String::from_utf8(v.0).unwrap()), v.1))
        })
        .collect()
}

/// ## Description
/// This function updates a user's amount of staked LP tokens as well as the accumulated token rewards.
///
/// * **user** is an object of type [`UserInfo`]. This is the user for which we update LP and reward related amounts.
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator in which the user is staked.
///
/// * **amount** is a variable of type [`Uint128`]. This is the new amount of LP tokens the user currently has staked.
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
