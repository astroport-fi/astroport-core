#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, CustomMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw1_whitelist::contract::{
    execute_execute, instantiate as cw1_instantiate, map_validate, query as cw1_query,
};
use cw1_whitelist::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use cw1_whitelist::state::ADMIN_LIST;
use cw1_whitelist::ContractError;
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::sudo::msg::TransferSudoMsg;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let resp = cw1_instantiate(deps.branch(), env, info, msg);
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    resp
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg<NeutronMsg>,
) -> Result<Response<NeutronMsg>, ContractError> {
    match msg {
        ExecuteMsg::Execute { msgs } => execute_execute(deps, env, info, msgs),
        ExecuteMsg::Freeze {} => execute_freeze(deps, info),
        ExecuteMsg::UpdateAdmins { admins } => execute_update_admins(deps, info, admins),
    }
}

pub fn execute_freeze<T: CustomMsg>(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response<T>, ContractError> {
    let mut cfg = ADMIN_LIST.load(deps.storage)?;
    if !cfg.can_modify(info.sender.as_ref()) {
        Err(ContractError::Unauthorized {})
    } else {
        cfg.mutable = false;
        ADMIN_LIST.save(deps.storage, &cfg)?;

        Ok(Response::default().add_attribute("action", "freeze"))
    }
}

pub fn execute_update_admins<T: CustomMsg>(
    deps: DepsMut,
    info: MessageInfo,
    admins: Vec<String>,
) -> Result<Response<T>, ContractError> {
    let mut cfg = ADMIN_LIST.load(deps.storage)?;
    if !cfg.can_modify(info.sender.as_ref()) {
        Err(ContractError::Unauthorized {})
    } else {
        cfg.admins = map_validate(deps.api, &admins)?;
        ADMIN_LIST.save(deps.storage, &cfg)?;

        Ok(Response::default().add_attribute("action", "update_admins"))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    cw1_query(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: TransferSudoMsg) -> StdResult<Response> {
    // Whitelist doesn't need any custom callback logic for IBC transfer messages
    Ok(Response::new())
}
