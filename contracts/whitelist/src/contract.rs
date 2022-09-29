use cosmwasm_std::{
    entry_point, Addr, Api, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult,
};

use cw1_whitelist::contract::{execute as cw1_execute, query as cw1_query};
use cw1_whitelist::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use cw1_whitelist::state::{AdminList, ADMIN_LIST};
use cw1_whitelist::ContractError;
use cw2::set_contract_version;

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

pub fn validate_addresses(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
    admins.iter().map(|addr| api.addr_validate(addr)).collect()
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
