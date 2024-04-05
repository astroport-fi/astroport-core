use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::asset::AssetInfo;
use cosmwasm_std::{Addr, Order, Uint128};
use cw20::Cw20ReceiveMsg;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// [`AssetInfo`] of the token that's being vested
    pub vesting_token: AssetInfo,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Claim claims vested tokens and sends them to a recipient
    Claim {
        /// The address that receives the vested tokens
        recipient: Option<String>,
        /// The amount of tokens to claim
        amount: Option<Uint128>,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    /// Withdraws from current active schedule. Setups a new schedule with the remaining amount.
    WithdrawFromActiveSchedule {
        /// The account from which tokens will be withdrawn
        account: String,
        /// The address that receives the vested tokens
        recipient: Option<String>,
        /// The amount of tokens to withdraw
        withdraw_amount: Uint128,
    },
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the offer to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
}

/// This structure stores vesting information for a specific address that is getting tokens.
#[cw_serde]
pub struct VestingAccount {
    /// The address that is getting tokens
    pub address: String,
    /// The vesting schedules targeted at the `address`
    pub schedules: Vec<VestingSchedule>,
}

/// This structure stores parameters for a batch of vesting schedules.
#[cw_serde]
pub struct VestingInfo {
    /// The vesting schedules
    pub schedules: Vec<VestingSchedule>,
    /// The total amount of ASTRO already claimed
    pub released_amount: Uint128,
}

/// This structure stores parameters for a specific vesting schedule
#[cw_serde]
pub struct VestingSchedule {
    /// The start date for the vesting schedule
    pub start_point: VestingSchedulePoint,
    /// The end point for the vesting schedule
    pub end_point: Option<VestingSchedulePoint>,
}

/// This structure stores the parameters used to create a vesting schedule.
#[cw_serde]
#[derive(Copy)]
pub struct VestingSchedulePoint {
    /// The start time for the vesting schedule
    pub time: u64,
    /// The amount of tokens being vested
    pub amount: Uint128,
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract using a [`ConfigResponse`] object.
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about an address vesting tokens using a [`VestingAccountResponse`] object.
    #[returns(VestingAccountResponse)]
    VestingAccount { address: String },
    /// Returns a list of addresses that are vesting tokens using a [`VestingAccountsResponse`] object.
    #[returns(VestingAccountsResponse)]
    VestingAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    /// Returns the total unvested amount of tokens for a specific address.
    #[returns(Uint128)]
    AvailableAmount { address: String },
    /// Timestamp returns the current timestamp
    #[returns(u64)]
    Timestamp {},
}

/// This structure describes a custom struct used to return the contract configuration.
#[cw_serde]
pub struct ConfigResponse {
    /// Address allowed to set contract parameters
    pub owner: Addr,
    /// [`AssetInfo`] of the token being vested
    pub vesting_token: AssetInfo,
}

/// This structure describes a custom struct used to return vesting data about a specific vesting target.
#[cw_serde]
pub struct VestingAccountResponse {
    /// The address that's vesting tokens
    pub address: Addr,
    /// Vesting information
    pub info: VestingInfo,
}

/// This structure describes a custom struct used to return vesting data for multiple vesting targets.
#[cw_serde]
pub struct VestingAccountsResponse {
    /// A list of accounts that are vesting tokens
    pub vesting_accounts: Vec<VestingAccountResponse>,
}

/// This enum describes the types of sorting that can be applied to some piece of data
#[cw_serde]
pub enum OrderBy {
    Asc,
    Desc,
}

// We suppress this clippy warning because Order in cosmwasm doesn't implement Debug and
// PartialEq for usage in QueryMsg. We need to use our own OrderBy and convert the result to cosmwasm's Order
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

/// This structure describes migration message.
#[cw_serde]
pub struct MigrateMsg {
    /// Special migration message needed during the Hub move.
    /// Cw admin must be very cautious supplying correct converter contract.
    pub converter_contract: String,
}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
}
