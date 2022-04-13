use astroport::DecimalCheckedOps;

use crate::state::Config;
use astroport::generator::UserInfoV2;
use astroport::querier::query_token_balance;
use astroport_governance::voting_escrow::{get_total_voting_power, get_voting_power};
use cosmwasm_std::{Addr, Decimal, Deps, Env, StdResult, Uint128};
use std::cmp::min;
use std::str::FromStr;

/// Calculates emission boost amount for specified user and generator
///
/// **b_u = min(0.4 * b_u + 0.6 * S * (w_i / W), b_u)**
///
/// - b_u is the amount of LP tokens a user staked in a generator
///
/// - S is the total amount of LP tokens staked in a generator
/// - w_i is a user’s current vxASTRO balance
/// - W is the total amount of vxASTRO
pub(crate) fn update_virtual_amount(
    deps: Deps,
    env: &Env,
    cfg: &Config,
    mut user_info: UserInfoV2,
    account: &Addr,
    generator: &Addr,
) -> StdResult<UserInfoV2> {
    let mut user_vp = Uint128::zero();
    let mut total_vp = Uint128::zero();

    if let Some(voting_escrow) = &cfg.voting_escrow {
        user_vp = get_voting_power(deps.querier, voting_escrow, account)?;
        total_vp = get_total_voting_power(deps.querier, voting_escrow)?;
    }

    // calculates emission boost only for user who has the voting power
    if user_vp.is_zero() || total_vp.is_zero() {
        user_info.virtual_amount = Uint128::zero();
        return Ok(user_info);
    }

    let total_balance = query_token_balance(
        &deps.querier,
        generator.clone(),
        env.contract.address.clone(),
    )?;

    let user_share_emission = Decimal::from_str("0.4")?.checked_mul(user_info.amount)?;
    let total_share_emission = Decimal::from_str("0.6")?.checked_mul(total_balance)?;
    let vx_share_emission = Decimal::from_ratio(user_vp, total_vp);

    let virtual_amount =
        user_share_emission.checked_add(vx_share_emission.checked_mul(total_share_emission)?)?;
    user_info.virtual_amount = min(virtual_amount, user_info.amount);

    Ok(user_info)
}
