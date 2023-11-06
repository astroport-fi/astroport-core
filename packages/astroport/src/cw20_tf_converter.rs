use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;

/// Holds the parameters used for creating a conversion contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The contract owner
    pub owner: String,
}

/// The contract migration message
/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}

/// Describes the execute messages available in the contract
#[cw_serde]
pub enum ExecuteMsg {}

/// Messages handled via CW20 transfers
#[cw_serde]
pub enum Cw20HookMsg {}

/// Describes the query messages available in the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}

/// The config of the conversion contract
#[cw_serde]
pub struct Config {
    /// The owner of the contract
    pub owner: Addr,
}
