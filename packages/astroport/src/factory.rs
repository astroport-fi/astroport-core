use crate::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{Addr, Binary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairType {
    Xyk {},
    Stable {},
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
pub struct PairConfig {
    pub code_id: u64,
    pub pair_type: PairType,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
    pub is_disabled: Option<bool>,
}

impl PairConfig {
    pub fn valid_fee_bps(&self) -> bool {
        self.total_fee_bps <= 10_000 && self.maker_fee_bps <= 10_000
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Pair contract code IDs which are allowed to create pairs
    pub pair_configs: Vec<PairConfig>,
    pub token_code_id: u64,
    /// Contract address to send fees to
    pub fee_address: Option<String>,
    /// Used for auto_stake from pools
    pub generator_address: String,
    /// Controls settings for factory, pools and tokenomics contracts
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        owner: Option<String>,
        token_code_id: Option<u64>,
        fee_address: Option<String>,
        generator_address: Option<String>,
    },
    UpdatePairConfig {
        config: PairConfig,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Type of pair contract
        pair_type: PairType,
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },
    Deregister {
        asset_infos: [AssetInfo; 2],
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Pairs {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
    FeeInfo {
        pair_type: PairType,
    },
}

// We define a custom struct for each query response
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
