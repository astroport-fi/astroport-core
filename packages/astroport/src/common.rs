use crate::asset::addr_validate_to_lower;
use cosmwasm_std::{attr, Addr, DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a request for a change of ownership.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OwnershipProposal {
    /// a new ownership.
    pub owner: Addr,
    /// time to live a request
    pub ttl: u64,
}

/// ## Description
/// Creates a new request to change ownership. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `new_owner` is a new owner.
///
/// `expires_in` is the validity period of the offer to change the owner.
///
/// `owner` is the current owner.
///
/// `proposal` is the object of type [`OwnershipProposal`].
pub fn propose_new_owner(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    new_owner: String,
    expires_in: u64,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response> {
    // permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let new_owner = addr_validate_to_lower(deps.api, new_owner.as_str())?;

    // check that owner is not the same
    if new_owner == owner {
        return Err(StdError::generic_err("New owner cannot be same"));
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

/// ## Description
/// Removes a request to change ownership. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `owner` is the current owner.
///
/// `proposal` is the object of type [`OwnershipProposal`].
pub fn drop_ownership_proposal(
    deps: DepsMut,
    info: MessageInfo,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response> {
    // permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    proposal.remove(deps.storage);

    Ok(Response::new().add_attributes(vec![attr("action", "drop_ownership_proposal")]))
}

/// ## Description
/// Approves owner. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `proposal` is the object of type [`OwnershipProposal`].
///
/// `cb` is a type of callback function that takes two parameters of type [`DepsMut`] and [`Addr`].
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

    // Check sender
    if info.sender != p.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if env.block.time.seconds() > p.ttl {
        return Err(StdError::generic_err("Ownership proposal expired"));
    }

    proposal.remove(deps.storage);

    // run callback
    cb(deps, p.owner.clone())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "claim_ownership"),
        attr("new_owner", p.owner),
    ]))
}
