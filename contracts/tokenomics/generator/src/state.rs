use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
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
    pub acc_per_share: Decimal,
    pub reward_proxy: Option<Addr>,
    pub acc_per_share_on_proxy: Decimal,
    // for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    // The ASTRO TOKEN!
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
    SendOrphanProxyReward {
        recipient: Addr,
        lp_token: Addr,
    },
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");
pub const TMP_USER_ACTION: Item<Option<ExecuteOnReply>> = Item::new("tmp_user_action");

// first key part is token, second - depositor
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");
