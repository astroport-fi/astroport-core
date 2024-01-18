#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdResult, Uint128};

use astroport::tokenfactory_tracker::QueryMsg;

use crate::state::{BALANCES, TOTAL_SUPPLY_HISTORY};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::BalanceAt { address, timestamp } => to_json_binary(&balance_at(
            deps,
            address,
            timestamp.unwrap_or_else(|| env.block.time.seconds()),
        )?),
        QueryMsg::TotalSupplyAt { timestamp } => to_json_binary(&total_supply_at(
            deps,
            timestamp.unwrap_or_else(|| env.block.time.seconds()),
        )?),
    }
}

fn balance_at(deps: Deps, address: String, timestamp: u64) -> StdResult<Uint128> {
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, timestamp)?
        .unwrap_or_default();
    Ok(balance)
}

fn total_supply_at(deps: Deps, timestamp: u64) -> StdResult<Uint128> {
    TOTAL_SUPPLY_HISTORY
        .may_load_at_height(deps.storage, timestamp)
        .map(|res| res.unwrap_or_default())
}
