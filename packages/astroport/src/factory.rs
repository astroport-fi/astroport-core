use crate::asset::{AssetInfo, PairInfo};

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};
use std::fmt::{Display, Formatter, Result};

const MAX_TOTAL_FEE_BPS: u16 = 10_000;
const MAX_MAKER_FEE_BPS: u16 = 10_000;

/// This structure holds the main contract parameters.
#[cw_serde]
pub struct Config {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Incentives contract address
    pub generator_address: Option<Addr>,
    /// Contract address to send governance fees to (the Maker contract)
    pub fee_address: Option<Addr>,
    /// CW1 whitelist contract code id used to store 3rd party incentives staking rewards
    pub whitelist_code_id: u64,
    /// The address of the contract that contains the coins with their precision
    pub coin_registry_address: Addr,
}

/// This enum describes available pair types.
/// ## Available pool types
/// ```
/// # use astroport::factory::PairType::{Custom, Stable, Xyk};
/// Xyk {};
/// Stable {};
/// Custom(String::from("Custom"));
/// ```
#[derive(Eq)]
#[cw_serde]
pub enum PairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Custom pair type
    Custom(String),
}

/// Returns a raw encoded string representing the name of each pool type
impl Display for PairType {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            PairType::Xyk {} => fmt.write_str("xyk"),
            PairType::Stable {} => fmt.write_str("stable"),
            PairType::Custom(pair_type) => fmt.write_str(format!("custom-{}", pair_type).as_str()),
        }
    }
}

/// This structure stores a pair type's configuration.
#[cw_serde]
pub struct PairConfig {
    /// ID of contract which is allowed to create pairs of this type
    pub code_id: u64,
    /// The pair type (provided in a [`PairType`])
    pub pair_type: PairType,
    /// The total fees (in bps) charged by a pair of this type
    pub total_fee_bps: u16,
    /// The amount of fees (in bps) collected by the Maker contract from this pair type
    pub maker_fee_bps: u16,
    /// Whether a pair type is disabled or not. If it is disabled, new pairs cannot be
    /// created, but existing ones can still read the pair configuration
    /// Default is false.
    #[serde(default)]
    pub is_disabled: bool,
    /// Setting this to true means that pairs of this type will not be able
    /// to get an ASTRO generator
    /// Default is false.
    #[serde(default)]
    pub is_generator_disabled: bool,
    /// If pool type is permissioned, only factory owner can create pairs of this type.
    /// Default is false.
    #[serde(default)]
    pub permissioned: bool,
}

impl PairConfig {
    /// This method is used to check fee bps.
    pub fn valid_fee_bps(&self) -> bool {
        self.total_fee_bps <= MAX_TOTAL_FEE_BPS && self.maker_fee_bps <= MAX_MAKER_FEE_BPS
    }
}

/// This structure stores the basic settings for creating a new factory contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// IDs of contracts that are allowed to instantiate pairs
    pub pair_configs: Vec<PairConfig>,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Contract address to send governance fees to (the Maker)
    pub fee_address: Option<String>,
    /// Address of contract that is used to auto_stake LP tokens once someone provides liquidity in a pool
    pub generator_address: Option<String>,
    /// Address of owner that is allowed to change factory contract parameters
    pub owner: String,
    /// CW1 whitelist contract code id used to store 3rd party rewards for staking Astroport LP tokens
    pub whitelist_code_id: u64,
    /// The address of the contract that contains the coins and their accuracy
    pub coin_registry_address: String,
    /// Config for the tracking contract
    pub tracker_config: Option<TrackerConfig>,
}

/// This structure describes the execute messages of the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        /// CW20 token contract code identifier
        token_code_id: Option<u64>,
        /// Contract address to send governance fees to (the Maker)
        fee_address: Option<String>,
        /// Contract address where Lp tokens can be auto_staked after someone provides liquidity in an incentivized Astroport pool
        generator_address: Option<String>,
        /// CW1 whitelist contract code id used to store 3rd party rewards for staking Astroport LP tokens
        whitelist_code_id: Option<u64>,
        /// The address of the contract that contains the coins and their accuracy
        coin_registry_address: Option<String>,
    },
    UpdateTrackerConfig {
        /// Tracking contract code id
        tracker_code_id: u64,
        /// Token factory module address
        token_factory_addr: Option<String>,
    },
    /// UpdatePairConfig updates the config for a pair type.
    UpdatePairConfig {
        /// New [`PairConfig`] settings for a pair type
        config: PairConfig,
    },
    /// CreatePair instantiates a new pair contract.
    CreatePair {
        /// The pair type (exposed in [`PairType`])
        pair_type: PairType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },
    /// Deregister removes a previously created pair.
    Deregister {
        /// The assets for which we deregister a pool
        asset_infos: Vec<AssetInfo>,
    },
    /// ProposeNewOwner creates a proposal to change contract ownership.
    /// The validity period for the proposal is set in the `expires_in` variable.
    ProposeNewOwner {
        /// Newly proposed contract owner
        owner: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer to change contract ownership.
    DropOwnershipProposal {},
    /// Used to claim contract ownership.
    ClaimOwnership {},
}

/// This structure describes the available query messages for the factory contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns contract settings specified in the custom [`ConfigResponse`] structure.
    #[returns(ConfigResponse)]
    Config {},
    /// Pair returns information about a specific pair according to the specified assets.
    #[returns(PairInfo)]
    Pair {
        /// The assets for which we return a pair
        asset_infos: Vec<AssetInfo>,
    },
    /// Pairs returns an array of pairs and their information according to the specified parameters in `start_after` and `limit` variables.
    #[returns(PairsResponse)]
    Pairs {
        /// The pair item to start reading from. It is an [`Option`] type that accepts [`AssetInfo`] elements.
        start_after: Option<Vec<AssetInfo>>,
        /// The number of pairs to read and return. It is an [`Option`] type.
        limit: Option<u32>,
    },
    /// FeeInfo returns fee parameters for a specific pair. The response is returned using a [`FeeInfoResponse`] structure
    #[returns(FeeInfoResponse)]
    FeeInfo {
        /// The pair type for which we return fee information. Pair type is a [`PairType`] struct
        pair_type: PairType,
    },
    /// Returns a vector that contains blacklisted pair types
    #[returns(Vec<PairType>)]
    BlacklistedPairTypes {},
    #[returns(TrackerConfig)]
    TrackerConfig {},
}

#[cw_serde]
pub struct MigrateMsg {
    pub tracker_config: Option<TrackerConfig>,
}

/// A custom struct for each query response that returns general contract settings/configs.
#[cw_serde]
pub struct ConfigResponse {
    /// Addres of owner that is allowed to change contract parameters
    pub owner: Addr,
    /// IDs of contracts which are allowed to create pairs
    pub pair_configs: Vec<PairConfig>,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Address of contract to send governance fees to (the Maker)
    pub fee_address: Option<Addr>,
    /// Address of contract used to auto_stake LP tokens for Astroport pairs that are incentivized
    pub generator_address: Option<Addr>,
    /// CW1 whitelist contract code id used to store 3rd party rewards for staking Astroport LP tokens
    pub whitelist_code_id: u64,
    /// The address of the contract that contains the coins and their accuracy
    pub coin_registry_address: Addr,
}

/// A custom struct for each query response that returns an array of objects of type [`PairInfo`].
#[cw_serde]
pub struct PairsResponse {
    /// Arrays of structs containing information about multiple pairs
    pub pairs: Vec<PairInfo>,
}

/// A custom struct for each query response that returns an object of type [`FeeInfoResponse`].
#[cw_serde]
pub struct FeeInfoResponse {
    /// Contract address to send governance fees to
    pub fee_address: Option<Addr>,
    /// Total amount of fees (in bps) charged on a swap
    pub total_fee_bps: u16,
    /// Amount of fees (in bps) sent to the Maker contract
    pub maker_fee_bps: u16,
}

/// This is an enum used for setting and removing a contract address.
#[cw_serde]
pub enum UpdateAddr {
    /// Sets a new contract address.
    Set(String),
    /// Removes a contract address.
    Remove {},
}

#[cw_serde]
pub struct TrackerConfig {
    /// Tracking contract code id
    pub code_id: u64,
    /// Token factory module address
    pub token_factory_addr: String,
}
