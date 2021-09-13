use cosmwasm_std::{Addr, Uint128, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub astro_token: String,
    pub tokens_per_block: Uint128,
    pub start_block: Uint64,
    pub bonus_end_block: Uint64,
    pub allowed_reward_proxies: Vec<String>,
    pub vesting_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Add {
        lp_token: Addr,
        alloc_point: Uint64,
        with_update: bool,
        reward_proxy: Option<String>,
    },
    Set {
        lp_token: Addr,
        alloc_point: Uint64,
        with_update: bool,
    },
    MassUpdatePools {},
    UpdatePool {
        lp_token: Addr,
    },
    Deposit {
        lp_token: Addr,
        amount: Uint128,
    },
    Withdraw {
        lp_token: Addr,
        amount: Uint128,
    },
    EmergencyWithdraw {
        lp_token: Addr,
    },
    SetAllowedRewardProxies {
        proxies: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    PoolLength {},
    Deposit { lp_token: Addr, user: Addr },
    PendingToken { lp_token: Addr, user: Addr },
    GetMultiplier { from: Uint64, to: Uint64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolLengthResponse {
    pub length: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetMultiplierResponse {
    pub multiplier: Uint64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingTokenResponse {
    pub pending: Uint128,
    pub pending_on_proxy: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
