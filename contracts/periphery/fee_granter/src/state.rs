use astroport::common::OwnershipProposal;
use astroport::fee_granter::Config;
use cosmwasm_std::{Addr, Api, StdError, StdResult, Uint128};
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");

pub const GRANTS: Map<&Addr, Uint128> = Map::new("grants");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub const MAX_ADMINS: usize = 2;

pub fn validate_admins(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
    if admins.len() > MAX_ADMINS {
        return Err(StdError::generic_err(format!(
            "Maximum allowed number of admins is {MAX_ADMINS}"
        )));
    }

    admins.iter().map(|addr| api.addr_validate(addr)).collect()
}
