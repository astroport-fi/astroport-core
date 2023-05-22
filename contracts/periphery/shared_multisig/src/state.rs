use cosmwasm_std::{StdResult, Storage};

use astroport::common::OwnershipProposal;
use astroport::shared_multisig::{Config, MultisigRole};
use cw3::{Proposal, Vote};
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

pub const BALLOTS: Map<(u64, &MultisigRole), Vote> = Map::new("votes");
pub const PROPOSALS: Map<u64, Proposal> = Map::new("proposals");

/// Contains a proposal to change contract Manager.
pub const MANAGER_PROPOSAL: Item<OwnershipProposal> = Item::new("manager_proposal");

/// Contains a proposal to change contract DAO.
pub const DAO_PROPOSAL: Item<OwnershipProposal> = Item::new("dao_proposal");

// settings for pagination
pub const MAX_LIMIT: u32 = 30;
pub const DEFAULT_LIMIT: u32 = 10;

pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}
