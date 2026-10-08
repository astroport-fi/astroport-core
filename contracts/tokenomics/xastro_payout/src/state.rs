use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

use astroport::common::OwnershipProposal;

use crate::msg::{Config, Status};

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATUS: Item<Status> = Item::new("status");
/// Amount paid to each address
pub const PAID: Map<&Addr, Uint128> = Map::new("paid");
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
