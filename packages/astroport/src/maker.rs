use crate::asset::{Asset, AssetInfo};
use crate::factory::UpdateAddr;
use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores general parameters for the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address that's allowed to change contract parameters
    pub owner: String,
    /// The ASTRO token contract address
    pub astro_token_contract: String,
    /// The factory contract address
    pub factory_contract: String,
    /// The xASTRO staking contract address
    pub staking_contract: String,
    /// The governance contract address (fee distributor for vxASTRO)
    pub governance_contract: Option<String>,
    /// The percentage of fees that go to governance_contract
    pub governance_percent: Option<Uint64>,
    /// The maximum spread used when swapping fee tokens to ASTRO
    pub max_spread: Option<Decimal>,
}

/// This structure describes the functions that can be executed in this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Collects and swaps fee tokens to ASTRO
    Collect {
        /// The assets to swap to ASTRO
        assets: Vec<AssetWithLimit>,
    },
    /// Updates general settings
    UpdateConfig {
        /// The factory contract address
        factory_contract: Option<String>,
        /// The xASTRO staking contract address
        staking_contract: Option<String>,
        /// The governance contract address (fee distributor for vxASTRO)
        governance_contract: Option<UpdateAddr>,
        /// The percentage of fees that go to governance_contract
        governance_percent: Option<Uint64>,
        /// The maximum spread used when swapping fee tokens to ASTRO
        max_spread: Option<Decimal>,
    },
    /// Add bridge tokens used to swap specific fee tokens to ASTRO (effectively declaring a swap route)
    UpdateBridges {
        add: Option<Vec<(AssetInfo, AssetInfo)>>,
        remove: Option<Vec<AssetInfo>>,
    },
    /// Swap fee tokens via bridge assets
    SwapBridgeAssets { assets: Vec<AssetInfo>, depth: u64 },
    /// Distribute ASTRO to stakers and to governance
    DistributeAstro {},
    /// Creates a request to change the contract's ownership
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    DropOwnershipProposal {},
    /// Claims contract ownership
    ClaimOwnership {},
    /// Enables the distribution of current fees accrued in the contract over "blocks" number of blocks
    EnableRewards { blocks: u64 },
}

/// This structure describes the query functions available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about the maker configs that contains in the [`ConfigResponse`]
    Config {},
    /// Returns the balance for each asset in the specified input parameters
    Balances {
        assets: Vec<AssetInfo>,
    },
    Bridges {},
}

/// A custom struct that holds contract parameters and is used to retrieve them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that is allowed to update contract parameters
    pub owner: Addr,
    /// The ASTRO token contract address
    pub astro_token_contract: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The xASTRO staking contract address
    pub staking_contract: Addr,
    /// The governance contract address (fee distributor for vxASTRO stakers)
    pub governance_contract: Option<Addr>,
    /// The percentage of fees that go to governance_contract
    pub governance_percent: Uint64,
    /// The maximum spread used when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
    /// The remainder ASTRO tokens (accrued before the Maker is upgraded) to be distributed to xASTRO stakers
    pub remainder_reward: Uint128,
    /// The amount of ASTRO tokens accrued before upgrading the Maker implementation and enabling reward distribution
    pub pre_upgrade_astro_amount: Uint128,
}

/// A custom struct used to return multiple asset balances.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BalancesResponse {
    pub balances: Vec<Asset>,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This struct holds parameters to help with swapping a specific amount of a fee token to ASTRO.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetWithLimit {
    /// Information about the fee token to swap
    pub info: AssetInfo,
    /// The amount of tokens to swap
    pub limit: Option<Uint128>,
}
