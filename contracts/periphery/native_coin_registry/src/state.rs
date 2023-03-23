use astroport::common::OwnershipProposal;
use astroport::native_coin_registry::Config;
use cw_storage_plus::Item;

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Contains a proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
