use astroport::DecimalCheckedOps;

use crate::querier::query_generator_controller_info;
use crate::state::{Config, UserInfo};
use astroport::querier::query_token_balance;
use astroport_governance::voting_escrow::{get_total_voting_power, get_voting_power};
use cosmwasm_std::{Addr, Decimal, DepsMut, Env, StdResult, Uint128};
use std::cmp::min;
use std::str::FromStr;

// b_u = min(0.4 * b_u + 0.6 * S * (w_i / W), b_u)
//
// - b_u is the amount of LP tokens a user staked in a generator
// - S is the total amount of LP tokens staked in a generator
// - w_i is a user’s current vxASTRO balance
// - W is the total amount of vxASTRO

/// Calculates boost amount for specified user and LP token
pub(crate) fn update_emission_rewards(
    mut deps: DepsMut,
    env: &Env,
    cfg: &Config,
    mut user_info: UserInfo,
    account: &Addr,
    generator: &Addr,
) -> StdResult<UserInfo> {
    let mut user_vp = Uint128::zero();
    let mut total_vp = Uint128::zero();

    if let Some(generator_controller) = &cfg.generator_controller {
        let escrow_addr =
            query_generator_controller_info(deps.branch(), generator_controller)?.escrow_addr;
        user_vp = get_voting_power(deps.querier, &escrow_addr, account)?;
        total_vp = get_total_voting_power(deps.querier, &escrow_addr)?;
    }

    // calculates emission boost only for user who has the voting power
    if user_vp.is_zero() {
        user_info.emission_amount = Uint128::zero();
        return Ok(user_info);
    }

    let total_balance = query_token_balance(
        &deps.querier,
        generator.clone(),
        env.contract.address.clone(),
    )?;

    let user_emission = Decimal::from_str("0.4")?.checked_mul(user_info.amount)?;

    let vx_emission;
    if !total_vp.is_zero() {
        vx_emission = Decimal::from_ratio(user_vp, total_vp);
    } else {
        vx_emission = Decimal::zero();
    }

    let total_emission = Decimal::from_str("0.6")?.checked_mul(total_balance)?;

    let val1 = user_emission.checked_add(vx_emission.checked_mul(total_emission)?)?;
    user_info.emission_amount = min(val1, user_info.amount);

    Ok(user_info)
}
