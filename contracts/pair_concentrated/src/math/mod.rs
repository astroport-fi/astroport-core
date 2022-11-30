use cosmwasm_std::{Decimal256, StdResult};

use crate::consts::N;
use crate::math::math_decimal::{geometric_mean, newton_d, newton_y};
use crate::state::AmpGamma;

mod math_decimal;
#[cfg(test)]
mod math_f64;
mod signed_decimal;

pub use math_decimal::half_float_pow;

pub fn calc_d(xs: &[Decimal256], amp_gamma: &AmpGamma) -> StdResult<Decimal256> {
    newton_d(xs, amp_gamma.amp.into(), amp_gamma.gamma.into())
}

pub fn calc_y(
    xs: &[Decimal256],
    d: Decimal256,
    amp_gamma: &AmpGamma,
    ask_ind: usize,
) -> StdResult<Decimal256> {
    newton_y(xs, amp_gamma.amp.into(), amp_gamma.gamma.into(), d, ask_ind)
}

/// * **d** - internal D invariant.
/// * **price_scale** - x_0/x_1 exchange rate.
pub fn get_xcp(d: Decimal256, price_scale: Decimal256) -> Decimal256 {
    let xs = [d / N, d / (N * price_scale)];
    geometric_mean(&xs)
}
