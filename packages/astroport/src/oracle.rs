use crate::asset::AssetInfo;
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the factory contract address
    pub factory_contract: String,
    /// the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Update prices
    Update {},
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Validates assets and calculates a new average amount with updated precision
    Consult {
        /// the assets to validate
        token: AssetInfo,
        /// the amount
        amount: Uint128,
    },
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
