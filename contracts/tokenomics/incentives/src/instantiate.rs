#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Uint128};

use astroport::asset::{addr_opt_validate, validate_native_denom};
use astroport::incentives::{Config, InstantiateMsg};

use crate::error::ContractError;
use crate::state::{ACTIVE_POOLS, CONFIG};

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.astro_token.check(deps.api)?;

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if let Some(fee_info) = &msg.incentivization_fee_info {
        deps.api.addr_validate(fee_info.fee_receiver.as_str())?;
        validate_native_denom(&fee_info.fee.denom)?;
    }

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            factory: deps.api.addr_validate(&msg.factory)?,
            generator_controller: None,
            astro_token: msg.astro_token,
            astro_per_second: Uint128::zero(),
            total_alloc_points: Uint128::zero(),
            vesting_contract: deps.api.addr_validate(&msg.vesting_contract)?,
            guardian: addr_opt_validate(deps.api, &msg.guardian)?,
            incentivization_fee_info: msg.incentivization_fee_info,
            token_transfer_gas_limit: None,
        },
    )?;
    ACTIVE_POOLS.save(deps.storage, &vec![])?;

    Ok(Response::new())
}
