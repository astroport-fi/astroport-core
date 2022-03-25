use astroport::DecimalCheckedOps;

use cosmwasm_std::{Decimal, StdResult, Uint128};
use std::cmp::min;
use std::str::FromStr;

pub(crate) fn calc_boost_amount(
    user_lp: Uint128,
    total_lp: Uint128,
    user_vp: Uint128,
    total_vp: Uint128,
) -> StdResult<Uint128> {
    let user_emission = Decimal::from_str("0.4")?.checked_mul(user_lp)?;
    let vx_emission = Decimal::from_ratio(user_vp, total_vp);
    let total_emission = Decimal::from_str("0.6")?.checked_mul(total_lp)?;

    Ok(min(
        user_emission.checked_add(vx_emission.checked_mul(total_emission)?)?,
        user_lp,
    ))
}
