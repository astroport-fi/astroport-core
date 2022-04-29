use astroport::common::OwnershipProposal;
use astroport::pair_reserve::ConfigResponse;
use cw_storage_plus::Item;

/// ## Description
/// This structure stores the main config parameters for a reserve pair contract.
pub type Config = ConfigResponse;

/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
