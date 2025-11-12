use crate::asset::{Asset, AssetInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, Uint128};
use std::ops::RangeInclusive;

/// Validation limit for max spread. From 0 to 50%.
pub const MAX_ALLOWED_SPREAD: Decimal = Decimal::percent(50);
/// Validation limits for a cooldown period. From 30 to 600 seconds.
pub const COOLDOWN_LIMITS: RangeInclusive<u64> = 30..=600;
/// Maximum allowed route hops
pub const MAX_SWAPS_DEPTH: u8 = 5;
/// Default pagination limit
pub const DEFAULT_PAGINATION_LIMIT: u32 = 50;

#[cw_serde]
pub struct DevFundConfig {
    /// The dev fund address
    pub address: String,
    /// The percentage of fees that go to the dev fund
    pub share: Decimal,
    /// Asset that devs want ASTRO to be swapped to
    pub asset_info: AssetInfo,
    /// Pair address to swap ASTRO to dev's reserve asset
    pub pool_addr: Addr,
}

/// This structure stores the main parameters for the Maker contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to set contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The dev fund configuration
    pub dev_fund_conf: Option<DevFundConfig>,
    /// ASTRO denom
    pub astro_denom: String,
    /// Address which receives all swapped Astro.
    /// On the Hub it simply sends astro to the staking contract;
    /// On an outpost - triggers IBC send on the satellite contract.
    pub collector: Addr,
    /// The maximum spread used when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
    /// If set defines the period when maker collect can be called
    pub collect_cooldown: Option<u64>,
}

#[cw_serde]
pub struct InstantiateMsg {
    /// The contract's owner, who can update config
    pub owner: String,
    /// The factory contract address
    pub factory_contract: String,
    /// ASTRO denom
    pub astro_denom: String,
    /// Address which receives all swapped Astro.
    /// On the Hub it simply sends astro to the staking contract;
    /// On an outpost - triggers IBC send on the satellite contract.
    pub collector: String,
    /// The maximum spread used when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
    /// If set defines the period when maker collect can be called
    pub collect_cooldown: Option<u64>,
}

#[cw_serde]
pub struct UpdateDevFundConfig {
    /// If 'set' is None then dev fund config will be removed,
    /// otherwise it will be updated with the new parameters
    pub set: Option<DevFundConfig>,
}

/// This structure describes the functions that can be executed in this contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Collects and swaps fee tokens to ASTRO
    Collect {
        /// The assets to swap to ASTRO
        assets: Vec<AssetWithLimit>,
    },
    /// Updates general settings
    UpdateConfig {
        /// ASTRO denom
        astro_denom: Option<String>,
        /// Address which receives all swapped Astro.
        /// On the Hub it simply sends astro to the staking contract;
        /// On an outpost - triggers IBC send on the satellite contract.
        collector: Option<String>,
        /// The maximum spread used when swapping fee tokens to ASTRO
        max_spread: Option<Decimal>,
        /// Defines the period when maker collect can be called
        collect_cooldown: Option<u64>,
        /// Dev tax configuration
        dev_fund_config: Option<Box<UpdateDevFundConfig>>,
    },
    /// Configure specific pool addresses for swapping asset_in to asset_out.
    /// If a route already exists, it will be overwritten.
    SetPoolRoutes(Vec<PoolRoute>),
    /// Self-call endpoint used to swap the whole asset_in balance.
    /// No one by Maker contract can call this endpoint.
    AutoSwap {
        asset_in: AssetInfo,
        asset_out: AssetInfo,
        pool_addr: Addr,
    },
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
    /// Permissionless endpoint that sends certain assets to predefined seizing address
    Seize {
        /// The assets to seize
        assets: Vec<AssetWithLimit>,
    },
    /// Sets parameters for seizing assets.
    /// Permissioned to a contract owner.
    /// If governance wants to stop seizing assets, it can set an empty list of seizable assets.
    UpdateSeizeConfig {
        /// The address that will receive the seized tokens
        receiver: Option<String>,
        /// The assets that can be seized. Resets the list to this one every time it is executed
        #[serde(default)]
        seizable_assets: Vec<AssetInfo>,
    },
}

/// This structure describes the query functions available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns information about the maker configs that contains in the [`Config`]
    #[returns(Config)]
    Config {},
    /// Returns the seize config
    #[returns(SeizeConfig)]
    QuerySeizeConfig {},
    /// Get a route for swapping an input asset into astro.
    /// Asset type (either native or cw20)
    /// will be determined automatically using [`asset::determine_asset_info`]
    #[returns(Vec<RouteStep>)]
    Route { asset_in: String },
    /// List all maker routes.
    /// Asset type (either native or cw20)
    /// will be determined automatically using [`asset::determine_asset_info`]
    #[returns(Vec<PoolRoute>)]
    Routes {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Return current spot price swapping In for Out
    #[returns(Uint128)]
    EstimateSwap { asset_in: Asset },
}

/// This struct holds parameters to help with swapping a specific amount of a fee token to ASTRO.
#[cw_serde]
pub struct AssetWithLimit {
    /// Information about the fee token to swap
    pub info: AssetInfo,
    /// The amount of tokens to swap
    pub limit: Option<Uint128>,
}

#[cw_serde]
pub struct SeizeConfig {
    /// The address of the contract that will receive the seized tokens
    pub receiver: Addr,
    /// The assets that can be seized
    pub seizable_assets: Vec<AssetInfo>,
}

#[cw_serde]
pub struct RouteStep {
    pub asset_out: AssetInfo,
    pub pool_addr: Addr,
}

#[cw_serde]
#[derive(Eq, Hash)]
pub struct PoolRoute {
    pub asset_in: AssetInfo,
    pub asset_out: AssetInfo,
    pub pool_addr: String,
}
