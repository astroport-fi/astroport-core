use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub token: Addr,
    pub dev_addr: Addr,
    pub tokens_per_block: Uint128,
    pub start_block: u64,
    pub bonus_end_block: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Add {
        alloc_point: u64,
        token: Addr,
    },
    Set {
        token: Addr,
        alloc_point: u64,
    },
    MassUpdatePools {},
    UpdatePool {
        token: Addr,
    },
    Deposit {
        token: Addr,
        amount: Uint128,
    },
    Withdraw {
        token: Addr,
        amount: Uint128,
    },
    EmergencyWithdraw {
        token: Addr,
    },
    SetDev {
        dev_address: Addr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    PoolLength {},
    PendingToken { token: Addr, user: Addr },
    GetMultiplier { from: u64, to: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolLengthResponse {
    pub length: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetMultiplierResponse {
    pub multiplier: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingTokenResponse {
    pub pending: Uint128,
}
