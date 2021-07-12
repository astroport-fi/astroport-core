use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{HumanAddr, Uint128, Order};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub owner: HumanAddr,
    pub token_addr: HumanAddr,
    pub genesis_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    UpdateConfig {
        owner: Option<HumanAddr>,
        token_addr: Option<HumanAddr>,
        genesis_time: Option<u64>,
    },
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    Claim {},
}

/// CONTRACT: end_time > start_time
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    pub address: HumanAddr,
    pub schedules: Vec<(u64, u64, Uint128)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingInfo {
    pub schedules: Vec<(u64, u64, Uint128)>,
    pub last_claim_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    VestingAccount {
        address: HumanAddr,
    },
    VestingAccounts {
        start_after: Option<HumanAddr>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: HumanAddr,
    pub token_addr: HumanAddr,
    pub genesis_time: u64,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountResponse {
    pub address: HumanAddr,
    pub info: VestingInfo,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountsResponse {
    pub vesting_accounts: Vec<VestingAccountResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    Asc,
    Desc,
}

impl Into<Order> for OrderBy {
    fn into(self) -> Order {
        if self == OrderBy::Asc {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}