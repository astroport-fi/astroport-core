//! Messages for Maker 2.x.
//!
//! They live here rather than in the shared `astroport` package, whose `astroport::maker` types
//! stay those of Maker 1.x, so upgrading the Maker doesn't change the package.

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};

use astroport::asset::{Asset, AssetInfo};
pub use astroport_router::msg::SwapOperation;

/// Validation limits for the collect cooldown, in seconds.
pub const COOLDOWN_LIMITS: std::ops::RangeInclusive<u64> = 30..=600;
/// The most legs a Maker can split its base asset into.
pub const MAX_LEGS: usize = 10;

/// What a leg does with its share once it's in the leg's output asset.
#[cw_serde]
pub enum LegAction {
    /// Send the share to an address.
    Send { recipient: String },
    /// Execute `contract` with `msg` and the share attached (native funds, or a CW20 `send` with
    /// `msg` as its hook), so the receiver knows it has been paid, e.g. a burner or a staking
    /// contract that distributes rewards.
    Deposit { contract: String, msg: Binary },
}

/// One destination for part of the collected fees.
#[cw_serde]
pub struct Leg {
    /// Share of the base asset this leg gets. All legs' shares add up to exactly 1.
    pub share: Decimal,
    /// Swap operations from the base asset into the leg's output asset, through the router.
    /// Empty to keep the base asset.
    #[serde(default)]
    pub route: Vec<SwapOperation>,
    pub action: LegAction,
}

/// The Maker's configuration.
#[cw_serde]
pub struct Config {
    /// Address allowed to change the configuration
    pub owner: Addr,
    /// Router 2.x contract used for every swap
    pub router: Addr,
    /// Asset every fee token is swapped into before it's split across the legs
    pub base_asset: AssetInfo,
    /// Most price impact a swap may have, relative to the route's rate for a tiny amount
    pub max_spread: Decimal,
    /// If set, collect can be called at most once per this many seconds
    pub collect_cooldown: Option<u64>,
    pub legs: Vec<Leg>,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    pub router: String,
    pub base_asset: AssetInfo,
    /// Defaults to 5%
    pub max_spread: Option<Decimal>,
    pub collect_cooldown: Option<u64>,
    pub legs: Vec<Leg>,
    /// Routes from fee tokens to the base asset
    #[serde(default)]
    pub routes: Vec<(AssetInfo, Vec<SwapOperation>)>,
}

/// A fee token to collect, and optionally the most of it to swap in this call.
#[cw_serde]
pub struct AssetWithLimit {
    pub info: AssetInfo,
    pub limit: Option<Uint128>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Swaps the given fee tokens into the base asset along their routes, then splits the base
    /// asset across the legs. Anyone can call it.
    Collect { assets: Vec<AssetWithLimit> },
    /// Internal: splits the base asset across the legs.
    Distribute {},
    /// Internal: records the Maker's balance of a leg's output asset before its swap.
    SnapshotLeg { leg: u32 },
    /// Internal: deposits what a leg's swap produced.
    DepositLeg { leg: u32 },
    /// Owner only. Leaves unset fields as they are.
    UpdateConfig {
        router: Option<String>,
        base_asset: Option<AssetInfo>,
        max_spread: Option<Decimal>,
        collect_cooldown: Option<u64>,
        legs: Option<Vec<Leg>>,
    },
    /// Owner only. Sets or removes fee token routes to the base asset.
    UpdateRoutes {
        #[serde(default)]
        add: Vec<(AssetInfo, Vec<SwapOperation>)>,
        #[serde(default)]
        remove: Vec<AssetInfo>,
    },
    /// Creates a request to change contract ownership. Owner only.
    ProposeNewOwner { owner: String, expires_in: u64 },
    /// Removes a request to change contract ownership. Owner only.
    DropOwnershipProposal {},
    /// Claims contract ownership. Only the proposed owner.
    ClaimOwnership {},
    /// Permissionless: sends the given seizable assets to the seize receiver.
    Seize { assets: Vec<AssetWithLimit> },
    /// Owner only. Resets the seizable assets to this list every time.
    UpdateSeizeConfig {
        receiver: Option<String>,
        #[serde(default)]
        seizable_assets: Vec<AssetInfo>,
    },
}

#[cw_serde]
pub struct SeizeConfig {
    /// Address that receives seized assets
    pub receiver: Addr,
    /// Assets that can be seized
    pub seizable_assets: Vec<AssetInfo>,
}

#[cw_serde]
pub struct RouteResponse {
    pub asset: AssetInfo,
    pub operations: Vec<SwapOperation>,
}

#[cw_serde]
pub struct BalancesResponse {
    pub balances: Vec<Asset>,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    /// Fee token routes to the base asset
    #[returns(Vec<RouteResponse>)]
    Routes {},
    /// The Maker's balances of the given assets (zero balances left out)
    #[returns(BalancesResponse)]
    Balances { assets: Vec<AssetInfo> },
    #[returns(SeizeConfig)]
    QuerySeizeConfig {},
}

/// Migrates a Maker 1.7.0 to 2.x. The owner and seize config are kept; the rest of the old
/// config (governance, second receiver, dev fund, pre-upgrade rewards) and the old bridges are
/// replaced by what's given here.
#[cw_serde]
pub struct MigrateMsg {
    pub router: String,
    pub base_asset: AssetInfo,
    /// Keeps the old max spread if not set
    pub max_spread: Option<Decimal>,
    /// Keeps the old cooldown if not set
    pub collect_cooldown: Option<u64>,
    pub legs: Vec<Leg>,
    #[serde(default)]
    pub routes: Vec<(AssetInfo, Vec<SwapOperation>)>,
}
