use astroport::tokenfactory_tracker::QueryMsg;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, Env, StdError, StdResult, Uint128, Uint64};

use crate::{error::ContractError, state::BALANCES};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::BalanceAt { address, timestamp } => balance_at(deps, env, address, timestamp),
        QueryMsg::TotalSupplyAt { timestamp } => total_supply_at(deps, env, timestamp),
    }
}

fn balance_at(deps: Deps, env: Env, address: String, timestamp: Uint64) -> StdResult<Binary> {
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, timestamp.u64())?
        .unwrap_or_default();
    to_binary(&balance)
}

fn total_supply_at(deps: Deps, env: Env, timestamp: Uint64) -> StdResult<Binary> {
    let amount = Uint128::one();
    to_binary(&amount)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::{Addr, Decimal};

    use super::*;

    #[test]
    fn query_amount() {}
}
