use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

use astroport::common::OwnershipProposal;
use astroport::fee_granter::Config;

pub const CONFIG: Item<Config> = Item::new("config");

pub const GRANTS: Map<&Addr, Uint128> = Map::new("grants");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub const MAX_ADMINS: usize = 2;
