#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, Reply, Response, SubMsgResult};

use crate::error::ContractError;

pub const POST_TRANSFER_REPLY_ID: u64 = 1;

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        // Caller context: either utils:claim_rewards() or utils:remove_reward_from_pool().
        // If cw20 token reverts the transfer, we bypass it silently.
        // This can happen in abnormal situations when cw20 contract was tweaked and broken.
        Reply {
            id: POST_TRANSFER_REPLY_ID,
            result: SubMsgResult::Err(err_msg),
        } => Ok(Response::new().add_attribute("transfer_error", err_msg)),
        _ => Err(ContractError::FailedToParseReply {}),
    }
}
