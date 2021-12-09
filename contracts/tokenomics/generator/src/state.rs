use astroport::common::OwnershipProposal;
use astroport::DecimalCheckedOps;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Info of each user.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    pub amount: Uint128,
    pub reward_debt: Uint128,
    pub reward_debt_proxy: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub alloc_point: Uint64,
    pub last_reward_block: Uint64,
    pub accumulated_rewards_per_share: Decimal,
    pub reward_proxy: Option<Addr>,
    pub accumulated_proxy_rewards_per_share: Decimal,
    // for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// Orphan proxy rewards which are left by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    // ASTRO token address
    pub astro_token: Addr,
    // ASTRO tokens created per block.
    pub tokens_per_block: Uint128,
    // Total allocation points. Must be the sum of all allocation points in all pools.
    pub total_alloc_point: Uint64,
    // The block number when ASTRO mining starts.
    pub start_block: Uint64,
    // List of allowed reward proxy contracts
    pub allowed_reward_proxies: Vec<Addr>,
    // Vesting contract from which rewards are received
    pub vesting_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteOnReply {
    MassUpdatePools {},
    Add {
        lp_token: Addr,
        alloc_point: Uint64,
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

pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");
pub const TMP_USER_ACTION: Item<Option<ExecuteOnReply>> = Item::new("tmp_user_action");

// first key part is token, second - depositor
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");
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
