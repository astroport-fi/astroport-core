use crate::asset::AssetInfo;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Uint128, Uint256};

/// This structure stores general parameters for the contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory contract address
    pub factory_contract: String,
    /// The assets that have a pool for which this contract provides price feeds
    pub asset_infos: Vec<AssetInfo>,
}

/// This structure describes the execute functions available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Update/accumulate prices
    Update {},
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Calculates a new TWAP with updated precision
    #[returns(Vec<(AssetInfo, Uint256)>)]
    Consult {
        /// The asset for which to compute a new TWAP value
        token: AssetInfo,
        /// The amount of tokens for which to compute the token price
        amount: Uint128,
    },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}
