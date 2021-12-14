use crate::asset::{Asset, AssetInfo};
use crate::factory::UpdateAddr;
use cosmwasm_std::{Addr, Decimal, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Sets contract address that used for controls settings for maker
    pub owner: String,
    /// Sets the ASTRO token contract address
    pub astro_token_contract: String,
    /// Sets the factory contract address
    pub factory_contract: String,
    /// Sets the staking contract address
    pub staking_contract: String,
    /// Sets the governance contract address
    pub governance_contract: Option<String>,
    /// Sets the governance percent
    pub governance_percent: Option<Uint64>,
    /// Sets the maximum spread
    pub max_spread: Option<Decimal>,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Collects astro tokens from the given pairs
    Collect {
        /// Sets the pairs contracts
        pair_addresses: Vec<Addr>,
    },
    /// Updates general settings that contains in the  [`Config`]
    UpdateConfig {
        /// Sets the factory contract address
        factory_contract: Option<String>,
        /// Sets the staking contract address
        staking_contract: Option<String>,
        /// Sets the governance contract address
        governance_contract: Option<UpdateAddr>,
        /// Sets the governance percent
        governance_percent: Option<Uint64>,
        /// Sets the maximum spread
        max_spread: Option<Decimal>,
    },
    /// Creates a request to change ownership.
    ProposeNewOwner {
        /// Sets a new owner
        owner: String,
        /// Sets the validity period of the offer to change the owner
        expires_in: u64,
    },
    /// Removes a request to change ownership.
    DropOwnershipProposal {},
    /// Approves ownership.
    ClaimOwnership {},
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about the maker configs that contains in the [`Config`]
    Config {},
    /// Returns the balance for each asset in the specified input parameters
    Balances { assets: Vec<AssetInfo> },
}

/// ## Description
/// A custom struct for each query response that returns controls settings of contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: Addr,
    /// Sets the ASTRO token contract address
    pub astro_token_contract: Addr,
    /// Sets the factory contract address
    pub factory_contract: Addr,
    /// Sets the staking contract address
    pub staking_contract: Addr,
    /// Sets the governance contract address
    pub governance_contract: Option<Addr>,
    /// Sets the governance percent
    pub governance_percent: Uint64,
    /// Sets the maximum spread
    pub max_spread: Decimal,
}

/// ## Description
/// A custom struct for each query response that returns the balance of asset.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BalancesResponse {
    pub balances: Vec<Asset>,
}
