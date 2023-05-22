use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, Addr, Api, CustomQuery, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use cw_storage_plus::Item;

const MAX_PROPOSAL_TTL: u64 = 1209600;

/// This structure describes the parameters used for creating a request for a change of contract ownership.
#[cw_serde]
pub struct OwnershipProposal {
    /// The newly proposed contract owner
    pub owner: Addr,
    /// Time until the proposal to change ownership expires
    pub ttl: u64,
}

/// Creates a new request to change contract ownership.
///
/// `new_owner` is the newly proposed owner.
///
/// `expires_in` is the time during which the ownership change proposal is still valid.
///
/// `owner` is the current owner.
///
/// ## Executor
/// Only the current contract owner can execute this.
pub fn propose_new_owner<C, T>(
    deps: DepsMut<C>,
    info: MessageInfo,
    env: Env,
    new_owner: String,
    expires_in: u64,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response<T>>
where
    C: CustomQuery,
{
    // Permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let new_owner = deps.api.addr_validate(new_owner.as_str())?;

    // Check that the new owner is not the same as the current one
    if new_owner == owner {
        return Err(StdError::generic_err("New owner cannot be same"));
    }

    if MAX_PROPOSAL_TTL < expires_in {
        return Err(StdError::generic_err(format!(
            "Parameter expires_in cannot be higher than {MAX_PROPOSAL_TTL}"
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

/// Removes a request to change contract ownership.
/// `owner` is the current contract owner.
///
/// ## Executor
/// Only the current owner can execute this.
pub fn drop_ownership_proposal<C, T>(
    deps: DepsMut<C>,
    info: MessageInfo,
    owner: Addr,
    proposal: Item<OwnershipProposal>,
) -> StdResult<Response<T>>
where
    C: CustomQuery,
{
    // Permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    proposal.remove(deps.storage);

    Ok(Response::new().add_attributes(vec![attr("action", "drop_ownership_proposal")]))
}

/// Claims ownership over the contract.
///
/// `cb` is a callback function to process ownership transition.
///
/// ## Executor
/// Only the newly proposed owner can execute this.
pub fn claim_ownership<C, T>(
    deps: DepsMut<C>,
    info: MessageInfo,
    env: Env,
    proposal: Item<OwnershipProposal>,
    cb: fn(DepsMut<C>, Addr) -> StdResult<()>,
) -> StdResult<Response<T>>
where
    C: CustomQuery,
{
    let p = proposal
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

/// Bulk validation and conversion between [`String`] -> [`Addr`] for an array of addresses.
/// If any address is invalid, the function returns [`StdError`].
pub fn validate_addresses(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
    admins.iter().map(|addr| api.addr_validate(addr)).collect()
}
