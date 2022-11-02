#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdError, StdResult};

use ap_whitelist::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use astroport::common::validate_addresses;
use cw1_whitelist::contract::{execute as cw1_execute, query as cw1_query};
use cw1_whitelist::state::{AdminList, ADMIN_LIST};
use cw1_whitelist::ContractError;
use cw2::{get_contract_version, set_contract_version};

// Version info for contract migration.
const CONTRACT_NAME: &str = "astroport-cw1-whitelist";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = AdminList {
        admins: validate_addresses(deps.api, &msg.admins)?,
        mutable: msg.mutable,
    };
    ADMIN_LIST.save(deps.storage, &cfg)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    cw1_execute(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    cw1_query(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-cw1-whitelist" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(StdError::generic_err("An error occurred during migration")),
        },
        _ => return Err(StdError::generic_err("An error occurred during migration")),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
