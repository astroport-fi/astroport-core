use crate::asset::addr_validate_to_lower;
use cosmwasm_std::{attr, Addr, Api, DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_PROPOSAL_TTL: u64 = 1209600;

/// This structure describes the parameters used for creating a request for a change of contract ownership.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OwnershipProposal {
    /// The newly proposed contract owner
    pub owner: Addr,
    /// Time until the proposal to change ownership expires
    pub ttl: u64,
}

/// Creates a new request to change contract ownership. Returns an [`Err`] on failure or returns the [`Response`]
/// with the specified attributes if the operation was successful.
/// ## Executor
/// Only the current contract owner can execute this.
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `new_owner` is the newly proposed owner.
///
/// `expires_in` is the time during which the ownership change proposal is still valid.
///
/// `owner` is the current owner.
///
/// `proposal` is an object of type [`OwnershipProposal`].
pub fn propose_new_owner(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    new_owner: String,
    expires_in: u64,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response> {
    // Permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let new_owner = addr_validate_to_lower(deps.api, new_owner.as_str())?;

    // Check that the new owner is not the same as the current one
    if new_owner == owner {
        return Err(StdError::generic_err("New owner cannot be same"));
    }

    if MAX_PROPOSAL_TTL < expires_in {
        return Err(StdError::generic_err(format!(
            "Parameter expires_in cannot be higher than {}",
            MAX_PROPOSAL_TTL
        )));
    }

    proposal.save(
        deps.storage,
        &OwnershipProposal {
            owner: new_owner.clone(),
            ttl: env.block.time.seconds() + expires_in,
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "propose_new_owner"),
        attr("new_owner", new_owner),
    ]))
}

/// Removes a request to change contract ownership. Returns an [`Err`] on failure or returns the [`Response`]
/// with the specified attributes if the operation was successful.
/// ## Executor
/// Only the current owner can execute this.
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `owner` is the current contract owner.
///
/// `proposal` is the object of type [`OwnershipProposal`].
pub fn drop_ownership_proposal(
    deps: DepsMut,
    info: MessageInfo,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response> {
    // Permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    proposal.remove(deps.storage);

    Ok(Response::new().add_attributes(vec![attr("action", "drop_ownership_proposal")]))
}

/// Claims ownership over the contract. Returns an [`Err`] on failure or returns the [`Response`]
/// with the specified attributes if the operation was successful.
/// ## Executor
/// Only the newly proposed owner can execute this.
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `proposal` is an object of type [`OwnershipProposal`].
///
/// `cb` is a callback function that takes in two parameters of type [`DepsMut`] and [`Addr`] respectively.
pub fn claim_ownership(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    proposal: Item<OwnershipProposal>,
    cb: fn(DepsMut, Addr) -> StdResult<()>,
) -> StdResult<Response> {
    let p: OwnershipProposal = proposal
        .load(deps.storage)
        .map_err(|_| StdError::generic_err("Ownership proposal not found"))?;

    // Check the sender
    if info.sender != p.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if env.block.time.seconds() > p.ttl {
        return Err(StdError::generic_err("Ownership proposal expired"));
    }

    proposal.remove(deps.storage);

    // Run callback
    cb(deps, p.owner.clone())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "claim_ownership"),
        attr("new_owner", p.owner),
    ]))
}

/// ## Description
/// Bulk validation and conversion between [`String`] -> [`Addr`] for an array of addresses.
/// If any address is invalid, the function returns [`StdError`].
pub fn validate_addresses(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
    admins
        .iter()
        .map(|addr| addr_validate_to_lower(api, addr))
        .collect()
}
