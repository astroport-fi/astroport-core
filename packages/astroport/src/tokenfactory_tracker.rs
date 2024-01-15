use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, Addr, Api, Coin, CustomQuery, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};

#[cw_serde]
pub enum SudoMsg {
    // Sudo endpoint called by chain before sending tokens
    // Errors returned by this endpoint will prevent the transaction from being sent
    BlockBeforeSend {
        // The address being sent from
        from: String,
        // The address being sent to
        to: String,
        // The amount and denom being sent
        amount: Coin,
    },
    // Sudo endpoint called by chain before sending tokens
    // Errors returned by this endpoint will NOT prevent the transaction from being sent
    TrackBeforeSend {
        // The address being sent from
        from: String,
        // The address being sent to
        to: String,
        // The amount and denom being sent
        amount: Coin,
    },
}

// Sudo endpoint called by chain before sending tokens
// Errors returned by this endpoint will prevent the transaction from being sent
pub fn block_before_send<C, T>(
    _deps: DepsMut<C>,
    _env: Env,
    // The address being sent from
    _from: String,
    // The address being sent to
    _to: String,
    // The amount and denom being sent
    _amount: Coin,
) -> StdResult<Response<T>>
where
    C: CustomQuery,
{
    Ok(Response::new())
}

// Sudo endpoint called by chain before sending tokens
// Errors returned by this endpoint will NOT prevent the transaction from being sent
pub fn track_before_send<C, T>(
    _deps: DepsMut<C>,
    _env: Env,
    // The address being sent from
    _from: String,
    // The address being sent to
    _to: String,
    // The amount and denom being sent
    _amount: Coin,
) -> StdResult<Response<T>>
where
    C: CustomQuery,
{
    // let config = CONFIG.load(deps.storage)?;

    // // Ensure the denom being sent is the tracked denom
    // // If this isn't checked, another token could be tracked with the same
    // // contract and that will skew the real numbers
    // if amount.denom != config.tracked_denom {
    //     return Err(ContractError::InvalidDenom {
    //         expected_denom: config.tracked_denom,
    //     });
    // }

    // // If the token is minted directly to an address, we don't need to subtract
    // // as the sender is the module address
    // if from != config.tokenfactory_module_address {
    //     BALANCES.update(
    //         deps.storage,
    //         &from,
    //         env.block.time.seconds(),
    //         |balance| -> StdResult<_> {
    //             Ok(balance.unwrap_or_default().checked_sub(amount.amount)?)
    //         },
    //     )?;
    // } else {
    //     // Minted new tokens
    //     TOTAL_SUPPLY_HISTORY.update(
    //         deps.storage,
    //         env.block.time.seconds(),
    //         |balance| -> StdResult<_> {
    //             Ok(balance.unwrap_or_default().checked_add(amount.amount)?)
    //         },
    //     )?;
    // }

    // // When burning tokens, the receiver is the token factory module address
    // // Sending tokens to the module address isn't allowed by the chain
    // if to != config.tokenfactory_module_address {
    //     BALANCES.update(
    //         deps.storage,
    //         &to,
    //         env.block.time.seconds(),
    //         |balance| -> StdResult<_> {
    //             Ok(balance.unwrap_or_default().checked_add(amount.amount)?)
    //         },
    //     )?;
    // } else {
    //     // Burned tokens
    //     TOTAL_SUPPLY_HISTORY.update(
    //         deps.storage,
    //         env.block.time.seconds(),
    //         |balance| -> StdResult<_> {
    //             Ok(balance.unwrap_or_default().checked_sub(amount.amount)?)
    //         },
    //     )?;
    // }

    Ok(Response::new())
}

// use cosmwasm_schema::cw_serde;
// use cosmwasm_std::{
//     attr, Addr, Api, CustomQuery, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
// };
// use cw_storage_plus::Item;

// const MAX_PROPOSAL_TTL: u64 = 1209600;

// /// This structure describes the parameters used for creating a request for a change of contract ownership.
// #[cw_serde]
// pub struct OwnershipProposal {
//     /// The newly proposed contract owner
//     pub owner: Addr,
//     /// Time until the proposal to change ownership expires
//     pub ttl: u64,
// }

// /// Creates a new request to change contract ownership.
// ///
// /// `new_owner` is the newly proposed owner.
// ///
// /// `expires_in` is the time during which the ownership change proposal is still valid.
// ///
// /// `owner` is the current owner.
// ///
// /// ## Executor
// /// Only the current contract owner can execute this.
// pub fn propose_new_owner<C, T>(
//     deps: DepsMut<C>,
//     info: MessageInfo,
//     env: Env,
//     new_owner: String,
//     expires_in: u64,
//     owner: Addr,
//     proposal: Item<OwnershipProposal>,
// ) -> StdResult<Response<T>>
// where
//     C: CustomQuery,
// {
//     // Permission check
//     if info.sender != owner {
//         return Err(StdError::generic_err("Unauthorized"));
//     }

//     let new_owner = deps.api.addr_validate(new_owner.as_str())?;

//     // Check that the new owner is not the same as the current one
//     if new_owner == owner {
//         return Err(StdError::generic_err("New owner cannot be same"));
//     }

//     if MAX_PROPOSAL_TTL < expires_in {
//         return Err(StdError::generic_err(format!(
//             "Parameter expires_in cannot be higher than {MAX_PROPOSAL_TTL}"
//         )));
//     }

//     proposal.save(
//         deps.storage,
//         &OwnershipProposal {
//             owner: new_owner.clone(),
//             ttl: env.block.time.seconds() + expires_in,
//         },
//     )?;

//     Ok(Response::new().add_attributes(vec![
//         attr("action", "propose_new_owner"),
//         attr("new_owner", new_owner),
//     ]))
// }

// /// Removes a request to change contract ownership.
// /// `owner` is the current contract owner.
// ///
// /// ## Executor
// /// Only the current owner can execute this.
// pub fn drop_ownership_proposal<C, T>(
//     deps: DepsMut<C>,
//     info: MessageInfo,
//     owner: Addr,
//     proposal: Item<OwnershipProposal>,
// ) -> StdResult<Response<T>>
// where
//     C: CustomQuery,
// {
//     // Permission check
//     if info.sender != owner {
//         return Err(StdError::generic_err("Unauthorized"));
//     }

//     proposal.remove(deps.storage);

//     Ok(Response::new().add_attributes(vec![attr("action", "drop_ownership_proposal")]))
// }

// /// Claims ownership over the contract.
// ///
// /// `cb` is a callback function to process ownership transition.
// ///
// /// ## Executor
// /// Only the newly proposed owner can execute this.
// pub fn claim_ownership<C, T>(
//     deps: DepsMut<C>,
//     info: MessageInfo,
//     env: Env,
//     proposal: Item<OwnershipProposal>,
//     cb: fn(DepsMut<C>, Addr) -> StdResult<()>,
// ) -> StdResult<Response<T>>
// where
//     C: CustomQuery,
// {
//     let p = proposal
//         .load(deps.storage)
//         .map_err(|_| StdError::generic_err("Ownership proposal not found"))?;

//     // Check the sender
//     if info.sender != p.owner {
//         return Err(StdError::generic_err("Unauthorized"));
//     }

//     if env.block.time.seconds() > p.ttl {
//         return Err(StdError::generic_err("Ownership proposal expired"));
//     }

//     proposal.remove(deps.storage);

//     // Run callback
//     cb(deps, p.owner.clone())?;

//     Ok(Response::new().add_attributes(vec![
//         attr("action", "claim_ownership"),
//         attr("new_owner", p.owner),
//     ]))
// }

// /// Bulk validation and conversion between [`String`] -> [`Addr`] for an array of addresses.
// /// If any address is invalid, the function returns [`StdError`].
// pub fn validate_addresses(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
//     admins.iter().map(|addr| api.addr_validate(addr)).collect()
// }
