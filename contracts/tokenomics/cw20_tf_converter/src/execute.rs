use astroport::cw20_tf_converter::ExecuteMsg;
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, SubMsg, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;

/// Exposes all the execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}
