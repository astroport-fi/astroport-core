use cosmwasm_std::{Addr, Api, StdError, StdResult, Uint128};
use cw_storage_plus::{Item, Map};
use std::collections::HashSet;

use astroport::common::{validate_addresses, OwnershipProposal};
use astroport::fee_granter::Config;

pub const CONFIG: Item<Config> = Item::new("config");

pub const GRANTS: Map<&Addr, Uint128> = Map::new("grants");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub const MAX_ADMINS: usize = 2;

pub fn update_admins_with_validation(
    api: &dyn Api,
    cur_admins: Vec<Addr>,
    add_admins: &[String],
    remove_admins: &[String],
) -> StdResult<Vec<Addr>> {
    let mut admins: HashSet<_> = cur_admins.into_iter().collect();
    validate_addresses(api, add_admins)?
        .iter()
        .try_for_each(|admin| {
            if !admins.insert(admin.clone()) {
                return Err(StdError::generic_err(format!(
                    "Admin {admin} already exists",
                )));
            };
            Ok(())
        })?;

    let remove_set: HashSet<_> = validate_addresses(api, remove_admins)?
        .into_iter()
        .collect();
    let new_admins: Vec<_> = admins.difference(&remove_set).cloned().collect();

    if new_admins.len() > MAX_ADMINS {
        Err(StdError::generic_err(format!(
            "Maximum allowed number of admins is {MAX_ADMINS}"
        )))
    } else {
        Ok(new_admins)
    }
}
