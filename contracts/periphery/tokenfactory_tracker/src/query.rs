#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdResult, Uint128};

use astroport::tokenfactory_tracker::{ConfigResponse, QueryMsg};

use crate::state::{BALANCES, CONFIG, TOTAL_SUPPLY_HISTORY};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::BalanceAt { address, unit } => {
            to_json_binary(&balance_at(deps, env, address, unit)?)
        }
        QueryMsg::TotalSupplyAt { unit } => to_json_binary(&total_supply_at(deps, env, unit)?),
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_json_binary(&ConfigResponse {
                tracked_denom: config.d,
                token_factory_module: config.m,
                track_over_seconds: config.t,
            })
        }
    }
}

fn balance_at(deps: Deps, env: Env, address: String, unit: Option<u64>) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let unit_now = if config.t {
        env.block.time.seconds()
    } else {
        env.block.height
    };

    match unit.unwrap_or(unit_now) {
        unit if unit == unit_now => BALANCES.may_load(deps.storage, &address),
        unit => BALANCES.may_load_at_height(deps.storage, &address, unit),
    }
    .map(|balance| balance.unwrap_or_default())
}

fn total_supply_at(deps: Deps, env: Env, unit: Option<u64>) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let unit_now = if config.t {
        env.block.time.seconds()
    } else {
        env.block.height
    };

    match unit.unwrap_or(unit_now) {
        unit if unit == unit_now => TOTAL_SUPPLY_HISTORY.may_load(deps.storage),
        unit => TOTAL_SUPPLY_HISTORY.may_load_at_height(deps.storage, unit),
    }
    .map(|total_supply| total_supply.unwrap_or_default())
}
