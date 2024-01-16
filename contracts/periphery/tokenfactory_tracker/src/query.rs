#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, Env, Order, StdResult, Uint128, Uint64};
use cw_storage_plus::Bound;

use astroport::tokenfactory_tracker::QueryMsg;

use crate::state::{BALANCES, TOTAL_SUPPLY_HISTORY};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::BalanceAt { address, timestamp } => to_binary(&balance_at(
            deps,
            address,
            timestamp.unwrap_or_else(|| Uint64::from(env.block.time.seconds())),
        )?),
        QueryMsg::TotalSupplyAt { timestamp } => to_binary(&total_supply_at(
            deps,
            timestamp.unwrap_or_else(|| Uint64::from(env.block.time.seconds())),
        )?),
    }
}

fn balance_at(deps: Deps, address: String, timestamp: Uint64) -> StdResult<Uint128> {
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, timestamp.u64())?
        .unwrap_or_default();
    Ok(balance)
}

fn total_supply_at(deps: Deps, timestamp: Uint64) -> StdResult<Uint128> {
    let end = Bound::inclusive(timestamp);
    let last_value_up_to_timestamp = TOTAL_SUPPLY_HISTORY
        .range(deps.storage, None, Some(end), Order::Descending)
        .next();

    if let Some(value) = last_value_up_to_timestamp {
        let (_, v) = value?;
        return Ok(v);
    }

    Ok(Uint128::zero())
}
