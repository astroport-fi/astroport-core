use crate::contract::Contract;

pub mod contract;
pub mod state;

use ap_pair_astro_xastro::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use ap_pair_bonded::{base::PairBonded, error::ContractError};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{from_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

use cw2::{get_contract_version, set_contract_version};

/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.init_params.is_none() {
        return Err(ContractError::InitParamsNotFound {});
    }

    let contract = Contract::new("params");
    contract.params.save(
        deps.storage,
        &from_binary(msg.init_params.as_ref().unwrap())?,
    )?;
    contract.instantiate(deps, env, info, msg)
}

/// Exposes all the execute functions available in the contract via a pair-bonded template.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let contract = Contract::new("params");
    contract.execute(deps, env, info, msg)
}

/// Exposes all the queries available in the contract via a pair-bonded template.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let contract = Contract::new("params");
    contract.query(deps, env, msg)
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-pair-astro-xastro" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(
        deps.storage,
        Contract::CONTRACT_NAME,
        Contract::CONTRACT_VERSION,
    )?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", Contract::CONTRACT_NAME)
        .add_attribute("new_contract_version", Contract::CONTRACT_VERSION))
}
