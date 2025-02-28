use cw_storage_plus::Item;

use astroport::common::OwnershipProposal;
use astroport_pcl_common::state::Config;

/// Stores pool parameters and state.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
