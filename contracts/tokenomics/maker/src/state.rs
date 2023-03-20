use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::maker::Config;
use cw_storage_plus::{Item, Map};

/// Stores the contract configuration at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Stores bridge tokens used to swap fee tokens to ASTRO
pub const BRIDGES: Map<String, AssetInfo> = Map::new("bridges");
