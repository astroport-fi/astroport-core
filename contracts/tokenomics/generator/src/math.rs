use astroport::DecimalCheckedOps;

use crate::state::USER_INFO;
use astroport_governance::voting_escrow::{get_total_voting_power, get_voting_power};
use cosmwasm_std::{Addr, Decimal, Deps, Env, StdResult, Uint128};
use cw20::BalanceResponse;
use std::cmp::min;
use std::str::FromStr;

/// Calculates boost amount for specified user and LP token
pub(crate) fn calc_boost_amount(
    deps: Deps,
    env: Env,
    user: &Addr,
    generator: &Addr,
    voting_escrow: &Addr,
) -> StdResult<Uint128> {
    let user_info = USER_INFO.load(deps.storage, (generator, &user))?;
    let res: BalanceResponse = deps.querier.query_wasm_smart(
        lp_token.clone(),
        &cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    let user_vp = get_voting_power(deps.querier, &voting_escrow, &user)?;
    let total_vp = get_total_voting_power(deps.querier, &voting_escrow)?;

    let user_emission = Decimal::from_str("0.4")?.checked_mul(user_lp)?;
    let vx_emission = Decimal::from_ratio(user_vp, total_vp);
    let total_emission = Decimal::from_str("0.6")?.checked_mul(total_lp)?;

    Ok(min(
        user_emission.checked_add(vx_emission.checked_mul(total_emission)?)?,
        user_lp,
    ))
}
