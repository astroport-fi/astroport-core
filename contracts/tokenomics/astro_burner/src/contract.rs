use cosmwasm_std::{
    coin, entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw2::{get_contract_version, set_contract_version};

use astroport::token_factory::tf_burn_msg;

use crate::error::ContractError;
use crate::msg::{Config, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{CONFIG, TOTAL_BURNED};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(deps.storage, &Config { denom: msg.denom })?;
    TOTAL_BURNED.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Burn {} => burn(deps, env, info),
    }
}

/// Burns the whole balance of the denom. Tokenfactory only lets the denom admin burn, so this
/// fails until the burner holds the admin.
fn burn(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // anything else sent here would be stuck
    if info.funds.iter().any(|c| c.denom != config.denom) {
        return Err(ContractError::UnexpectedFunds {
            denom: config.denom,
        });
    }

    let balance = deps
        .querier
        .query_balance(&env.contract.address, &config.denom)?
        .amount;
    if balance.is_zero() {
        return Ok(Response::new()
            .add_attribute("action", "burn")
            .add_attribute("burned", "0"));
    }

    TOTAL_BURNED
        .update::<_, ContractError>(deps.storage, |total| Ok(total.checked_add(balance)?))?;

    Ok(Response::new()
        .add_message(tf_burn_msg(
            env.contract.address,
            coin(balance.u128(), &config.denom),
        ))
        .add_attribute("action", "burn")
        .add_attribute("burned", balance))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::TotalBurned {} => to_json_binary(&TOTAL_BURNED.load(deps.storage)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(ContractError::MigrationError {});
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_version", version.version)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
