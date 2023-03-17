use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw_storage_plus::Map;

/// This structure stores the main parameters for the native coin registry contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
}

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Adds or updates native assets with specified precisions
    /// ## Executor
    /// Only the current owner can execute this
    Add { native_coins: Vec<(String, u8)> },
    /// Removes the native assets by specified parameters
    /// ## Executor
    /// Only the current owner can execute this
    Remove { native_coins: Vec<String> },
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

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract.
    #[returns(Config)]
    Config {},
    /// Returns the information about Asset by specified denominator.
    #[returns(CoinResponse)]
    NativeToken { denom: String },
    /// Returns a vector which contains the native assets.
    #[returns(Vec<CoinResponse>)]
    NativeTokens {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub struct CoinResponse {
    /// The asset name
    pub denom: String,
    /// The asset precision
    pub decimals: u8,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// The first key is denom, the second key is a precision.
pub const COINS_INFO: Map<String, u8> = Map::new("coins_info");
