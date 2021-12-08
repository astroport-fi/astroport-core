use crate::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{Addr, Binary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result};

/// ## Description
/// This enum describes available types of pair.
/// ## Available types
/// ```
/// # use astroport::factory::PairType::{Custom, Stable, Xyk};
/// Xyk {};
/// Stable {};
/// Custom(String);
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Custom pair type
    Custom(String),
}

// Provide a string version of this to raw encode strings
impl Display for PairType {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            PairType::Xyk {} => fmt.write_str("xyk"),
            PairType::Stable {} => fmt.write_str("stable"),
            PairType::Custom(pair_type) => fmt.write_str(format!("custom-{}", pair_type).as_str()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
/// ## Description
/// This structure describes a configuration of pair.
pub struct PairConfig {
    /// Sets pair contract code ID which are allowed to create pair
    pub code_id: u64,
    /// Sets the type of pair available in [`PairType`]
    pub pair_type: PairType,
    /// Sets a pair total fees bps
    pub total_fee_bps: u16,
    /// Sets a pair fees bps
    pub maker_fee_bps: u16,
    /// Used to check if pair configuration is disabled
    pub is_disabled: Option<bool>,
}

impl PairConfig {
    /// ## Description
    /// This method is used to check fee bps.
    /// ## Params
    /// `&self` is the type of the caller object.
    pub fn valid_fee_bps(&self) -> bool {
        self.total_fee_bps <= 10_000 && self.maker_fee_bps <= 10_000
    }
}

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Sets pair contract code IDs which are allowed to create pairs
    pub pair_configs: Vec<PairConfig>,
    /// Sets CW20 token contract code identifier
    pub token_code_id: u64,
    /// Sets contract address to send fees to
    pub fee_address: Option<String>,
    /// Sets contract address that used for auto_stake from pools
    pub generator_address: String,
    /// Sets contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        /// Sets contract address that used for controls settings for factory, pools and tokenomics contracts
        owner: Option<String>,
        /// Sets CW20 token contract code identifier
        token_code_id: Option<u64>,
        /// Sets contract address to send fees to
        fee_address: Option<String>,
        /// Sets contract address that used for auto_stake from pools
        generator_address: Option<String>,
    },
    /// UpdatePairConfig updates configs of pair
    UpdatePairConfig {
        /// Sets new [`PairConfig`] settings for pair
        config: PairConfig,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Sets the type of pair available in [`PairType`]
        pair_type: PairType,
        /// Sets the type of asset infos available in [`AssetInfo`]
        asset_infos: [AssetInfo; 2],
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },
    /// Removes a previously created pair
    Deregister {
        /// Sets the type of asset infos available in [`AssetInfo`]
        asset_infos: [AssetInfo; 2],
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns controls settings that specified in custom [`ConfigResponse`] structure
    Config {},
    /// Pair returns a pair according to the specified parameters in `asset_infos` variable.
    Pair { asset_infos: [AssetInfo; 2] },
    /// Pairs returns an array of pairs according to the specified parameters in `start_after` and `limit` variables.
    Pairs {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
    /// FeeInfo returns settings that specified in custom [`FeeInfoResponse`] structure
    FeeInfo { pair_type: PairType },
}

/// A custom struct for each query response that returns controls settings of contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: Addr,
    pub pair_configs: Vec<PairConfig>,
    pub token_code_id: u64,
    pub fee_address: Option<Addr>,
    pub generator_address: Addr,
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairsResponse {
    pub pairs: Vec<PairInfo>,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct FeeInfoResponse {
    pub fee_address: Option<Addr>,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAddr {
    Set(String),
    Remove {},
}
