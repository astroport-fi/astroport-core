use crate::contract::Contract;

pub mod contract;
pub mod state;

use crate::state::MigrateMsg;
use astroport::pair::InstantiateMsg;
use astroport::pair_bonded::{ExecuteMsg, QueryMsg};
use astroport_pair_bonded::base::PairBonded;
use astroport_pair_bonded::error::ContractError;
use cosmwasm_std::{
    entry_point, from_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
};

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters for creating the contract.
#[entry_point]
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

/// ## Description
/// Exposes all the execute functions available in the contract via a pair-bonded template.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let contract = Contract::new("params");
    contract.execute(deps, env, info, msg)
}

/// ## Description
/// Exposes all the queries available in the contract via a pair-bonded template.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let contract = Contract::new("params");
    contract.query(deps, env, msg)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[entry_point]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
