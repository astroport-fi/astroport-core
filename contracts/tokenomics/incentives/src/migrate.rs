#![cfg(not(tarpaulin_include))]

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Empty, Env, Response};

use crate::error::ContractError;
use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = cw2::get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-incentives" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" | "1.1.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
