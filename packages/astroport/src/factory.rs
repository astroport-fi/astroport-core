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
/// Custom(String::from("Custom"));
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
    /// pair contract code ID which are allowed to create pair
    pub code_id: u64,
    /// the type of pair available in [`PairType`]
    pub pair_type: PairType,
    /// a pair total fees bps
    pub total_fee_bps: u16,
    /// a pair fees bps
    pub maker_fee_bps: u16,
    /// We disable pair configs instead of removing them. If it is disabled, new pairs cannot be
    /// created, but existing ones can still obtain proper settings, such as fee amounts
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
    /// pair contract code IDs which are allowed to create pairs
    pub pair_configs: Vec<PairConfig>,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// contract address to send fees to
    pub fee_address: Option<String>,
    /// contract address that used for auto_stake from pools
    pub generator_address: Option<String>,
    /// contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: String,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        /// CW20 token contract code identifier
        token_code_id: Option<u64>,
        /// contract address to send fees to
        fee_address: Option<String>,
        /// contract address that used for auto_stake from pools
        generator_address: Option<String>,
    },
    /// UpdatePairConfig updates configs of pair
    UpdatePairConfig {
        /// new [`PairConfig`] settings for pair
        config: PairConfig,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// the type of pair available in [`PairType`]
        pair_type: PairType,
        /// the type of asset infos available in [`AssetInfo`]
        asset_infos: [AssetInfo; 2],
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },
    /// Deregister removes a previously created pair
    Deregister {
        /// the type of asset infos available in [`AssetInfo`]
        asset_infos: [AssetInfo; 2],
    },
    /// ProposeNewOwner creates an offer for a new owner. The validity period of the offer is set in the `expires_in` variable.
    ProposeNewOwner {
        /// contract address that used for controls settings for factory, pools and tokenomics contracts
        owner: String,
        /// the offer expiration date for the new owner
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer for the new owner.
    DropOwnershipProposal {},
    /// Used to claim(approve) new owner proposal, thus changing contract's owner
    ClaimOwnership {},
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns controls settings that specified in custom [`ConfigResponse`] structure
    Config {},
    /// Pair returns a pair according to the specified parameters in `asset_infos` variable.
    Pair {
        /// the type of asset infos available in [`AssetInfo`]
        asset_infos: [AssetInfo; 2],
    },
    /// Pairs returns an array of pairs according to the specified parameters in `start_after` and `limit` variables.
    Pairs {
        /// the item to start reading from. It is an [`Option`] type that accepts two [`AssetInfo`] elements.
        start_after: Option<[AssetInfo; 2]>,
        /// the number of items to be read. It is an [`Option`] type.
        limit: Option<u32>,
    },
    /// FeeInfo returns settings that specified in custom [`FeeInfoResponse`] structure
    FeeInfo {
        ///s the type of pair available in [`PairType`]
        pair_type: PairType,
    },
}

/// ## Description
/// A custom struct for each query response that returns controls settings of contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: Addr,
    /// Pair contract code IDs which are allowed to create pairs
    pub pair_configs: Vec<PairConfig>,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Contract address to send fees to
    pub fee_address: Option<Addr>,
    /// Contract address that used for auto_stake from pools
    pub generator_address: Option<Addr>,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// A custom struct for each query response that returns an array of objects type [`PairInfo`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairsResponse {
    pub pairs: Vec<PairInfo>,
}

/// ## Description
/// A custom struct for each query response that returns an object of type [`FeeInfoResponse`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct FeeInfoResponse {
    /// Contract address to send fees to
    pub fee_address: Option<Addr>,
    /// Pair total fees bps
    pub total_fee_bps: u16,
    /// Pair fees bps
    pub maker_fee_bps: u16,
}

/// ## Description
/// This is an enumeration for setting and unsetting a contract address.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAddr {
    /// Sets a new contract address.
    Set(String),
    /// Removes contract address.
    Remove {},
}
