use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Order, Timestamp, Uint128};
use cw20::Cw20ReceiveMsg;

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the token address
    pub token_addr: String,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Claims the amount from Vesting for transfer to the recipient
    Claim {
        /// the recipient of claim
        recipient: Option<String>,
        /// the amount of claim
        amount: Option<Uint128>,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received
    /// template.
    Receive(Cw20ReceiveMsg),
}

/// ## Description
/// This structure describes the basic settings for vesting account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    /// the address of account
    pub address: String,
    /// the schedules of account
    pub schedules: Vec<VestingSchedule>,
}

/// ## Description
/// This structure describes the basic settings for vesting information.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingInfo {
    /// the schedules
    pub schedules: Vec<VestingSchedule>,
    /// the released amount
    pub released_amount: Uint128,
}

/// ## Description
/// This structure describes the basic settings for vesting schedule.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingSchedule {
    /// the start point of schedule
    pub start_point: VestingSchedulePoint,
    /// the end point of schedule
    pub end_point: Option<VestingSchedulePoint>,
}

/// ## Description
/// This structure describes the basic settings for vesting schedule point.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingSchedulePoint {
    /// the time
    pub time: Timestamp,
    /// the amount
    pub amount: Uint128,
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// ## Description
    /// Returns information about the vesting configs in the [`ConfigResponse`] object.
    Config {},
    /// ## Description
    /// Returns information about the vesting account in the [`VestingAccountResponse`] object.
    VestingAccount { address: Addr },
    /// ## Description
    /// Returns a list of accounts, for the given input parameters, in the [`VestingAccountsResponse`] object.
    VestingAccounts {
        start_after: Option<Addr>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    /// ## Description
    /// Returns the available amount for specified account.
    AvailableAmount { address: Addr },
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// the token address
    pub token_addr: Addr,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountResponse {
    /// the token address
    pub address: Addr,
    /// the information object of type [`VestingInfo`]
    pub info: VestingInfo,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountsResponse {
    /// the vesting accounts information
    pub vesting_accounts: Vec<VestingAccountResponse>,
}

/// ## Description
/// This enum describes the type of sort
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    /// Ascending
    Asc,
    /// Descending
    Desc,
}

// We suppress this clippy warning because Order in cosmwasm doesn't implement Debug and
// PartialEq for usage in QueryMsg, we need to use our own OrderBy and
// convert it finally to cosmwasm's Order
#[allow(clippy::from_over_into)]
impl Into<Order> for OrderBy {
    fn into(self) -> Order {
        if self == OrderBy::Asc {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Register vesting accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
}
