use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;

/// This structure stores the main parameters for the generator vesting contract.
#[cw_serde]
pub struct Config {
    /// A coin to be wrapped
    pub denom: String,
    /// The token to be issued
    pub token: Addr,
}

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// A coin to be wrapped
    pub denom: String,
    /// CW20 token code identifier
    pub token_code_id: u64,
    /// The decimals value of the CW20 token.
    pub token_decimals: u8,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Wraps the specified native coin and issues a cw20 token instead.
    Wrap {},
    /// Receives a message of type [`Cw20ReceiveMsg`]
    /// Receives the specified cw20 token and issues a wrapped native coin in return.
    Receive(Cw20ReceiveMsg),
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract.
    #[returns(Config)]
    Config {},
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// Receives the specified cw20 token and issues a wrapped native coin in return.
    Unwrap {},
}
