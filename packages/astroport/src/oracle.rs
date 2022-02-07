use crate::asset::AssetInfo;
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure stores general parameters for the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The factory contract address
    pub factory_contract: String,
    /// The assets that have a pool for which this contract provides price feeds
    pub asset_infos: [AssetInfo; 2],
}

/// ## Description
/// This structure describes the execute functions available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Update/accumulate prices
    Update {},
}

/// ## Description
/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Calculates a new TWAP with updated precision
    Consult {
        /// The asset for which to compute a new TWAP value
        token: AssetInfo,
        /// The amount of tokens for which to compute the token price
        amount: Uint128,
    },
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
