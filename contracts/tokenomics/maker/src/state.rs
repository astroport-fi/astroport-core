use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};

use astroport::common::OwnershipProposal;

use crate::msg::{Config, SeizeConfig, SwapOperation};

/// Stores the contract configuration at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Fee token (by its asset string) -> swap operations to the base asset
pub const ROUTES: Map<String, Vec<SwapOperation>> = Map::new("routes");

/// Stores the latest timestamp when fees were collected
pub const LAST_COLLECT_TS: Item<u64> = Item::new("last_collect_ts");

/// Stores seize config
pub const SEIZE_CONFIG: Item<SeizeConfig> = Item::new("seize_config");

/// The Maker's balance of a leg's output asset right before that leg's swap, so the deposit
/// after the swap uses exactly what the swap produced
pub const LEG_SNAPSHOT: Item<Uint128> = Item::new("leg_snapshot");
