use cosmwasm_std::{Decimal256, Fraction, StdError, StdResult, Uint128};

use crate::consts::{HALFPOW_TOL, MAX_ITER, N, N_POW2, TOL};
use crate::math::signed_decimal::SignedDecimal256;
use itertools::Itertools;

/// Internal constant to increase calculation accuracy.
const PADDING: Decimal256 = Decimal256::raw(1e36 as u128);

pub fn geometric_mean(x: &[Decimal256]) -> Decimal256 {
    (x[0] * x[1]).sqrt()
}

pub(crate) fn f(
    d: SignedDecimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> SignedDecimal256 {
    let mul = x[0] * x[1];
    let d_pow2 = d.pow(2);

    let prod_n_n = mul * N_POW2;
    let k = a * gamma.pow(2) * prod_n_n
        / ((gamma + Decimal256::one() - prod_n_n / d_pow2).pow(2) * d_pow2);

    d * (x[0] + x[1]) * k + mul - k * d_pow2 - d_pow2 / N_POW2
}

/// df/dD
pub(crate) fn df_dd(
    d: SignedDecimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> SignedDecimal256 {
    let a_gamma_pow_2 = a * gamma.pow(2); // A * gamma^2
    let gamma_plus_1 = gamma + Decimal256::one();
    let d_pow_n = d.pow(2);
    let prod_n_n = x[0] * x[1] * N_POW2;
    let sum = x[0] + x[1];

    let k0 = prod_n_n / d_pow_n;
    let k0_prime = -SignedDecimal256::from(N) * prod_n_n;

    let gamma_one_k0 = gamma_plus_1 - k0; // gamma + 1 - K0

    let k = a_gamma_pow_2 * k0 / (gamma_plus_1 - k0).pow(2);
    let k_prime_numerator = PADDING * a_gamma_pow_2 * k0_prime * (gamma_plus_1 + k0);
    let k_prime_denominator = PADDING * d.pow(3) * gamma_one_k0 * gamma_one_k0 * gamma_one_k0;

    k_prime_numerator * d * sum / k_prime_denominator + k * sum
        - k_prime_numerator * d_pow_n / k_prime_denominator
        - N * k * d
        - d / N
}

pub(crate) fn newton_d(
    x: &[Decimal256],
    a: Decimal256,
    gamma: Decimal256,
) -> StdResult<Decimal256> {
    let mut d_prev: SignedDecimal256 = (N * geometric_mean(x)).into();
    let x = x.iter().map(SignedDecimal256::from).collect_vec();

    for _ in 0..MAX_ITER {
        let d = d_prev - f(d_prev, &x, a, gamma) / df_dd(d_prev, &x, a, gamma);
        if d.diff(d_prev) <= TOL {
            return d.try_into();
        }
        d_prev = d;
    }

    Err(StdError::generic_err("newton_d is not converging"))
}

/// df/dx
pub(crate) fn df_dx(
    d: Decimal256,
    x: &[SignedDecimal256],
    a: Decimal256,
    gamma: Decimal256,
    i: usize,
) -> SignedDecimal256 {
    let x_r = x[1 - i];
    let d_pow2 = d.pow(2);

    let k0 = x[0] * x[1] * N_POW2 / d_pow2;
    let gamma_one_k0 = gamma + Decimal256::one() - k0;
    let gamma_one_k0_pow2 = gamma_one_k0.pow(2);
    let a_gamma_pow2 = a * gamma.pow(2);

    let k = a_gamma_pow2 * k0 / gamma_one_k0_pow2;
    let k0_x = x_r * N_POW2;
    let k_x = k0_x * a_gamma_pow2 * (gamma + Decimal256::one() + k0) * PADDING
        / (PADDING * d_pow2 * gamma_one_k0 * gamma_one_k0_pow2);

    (k_x * (x[0] + x[1]) + k) * d + x_r - k_x * d_pow2
}

pub(crate) fn newton_y(
    xs: &[Decimal256],
    a: Decimal256,
    gamma: Decimal256,
    d: Decimal256,
    j: usize,
) -> StdResult<Decimal256> {
    let mut x = xs.iter().map(SignedDecimal256::from).collect_vec();
    let x0 = d.pow(2) / (N_POW2 * x[1 - j]);
    let mut xi_1 = x0;
    x[j] = x0;

    for _ in 0..MAX_ITER {
        let xi = xi_1 - f(d.into(), &x, a, gamma) / df_dx(d, &x, a, gamma, j);
        if xi.diff(xi_1) <= TOL {
            return xi.try_into();
        }
        x[j] = xi;
        xi_1 = xi;
    }

    Err(StdError::generic_err("newton_y is not converging"))
}

/// Calculates 0.5^power.
pub fn half_float_pow(power: Decimal256) -> StdResult<Decimal256> {
    let intpow = power.floor();
    let intpow_u128: Uint128 = (intpow.numerator() / intpow.denominator()).try_into()?;

    let half = Decimal256::from_ratio(1u8, 2u8);
    let frac_pow = power - intpow;

    // 0.5 ^ int_power
    let result = half.pow(intpow_u128.u128() as u32);

    let mut term = Decimal256::one();
    let mut sum = Decimal256::one();

    for i in 1..(MAX_ITER as u128) {
        let k = Decimal256::from_atomics(i, 0).unwrap();
        let mut c = k - Decimal256::one();

        c = frac_pow.abs_diff(c);
        term = term * c * half / k;
        sum -= term;

        if term < HALFPOW_TOL {
            return Ok(result * sum);
        }
    }

    Err(StdError::generic_err("halfpow is not converging"))
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;
    use std::str::FromStr;

    use anyhow::{anyhow, Result as AnyResult};

    use crate::math::math_f64::newton_d as newton_d_f64;
    use crate::math::math_f64::newton_y as newton_y_f64;

    use super::*;

    fn f64_to_dec(val: f64) -> Decimal256 {
        Decimal256::from_str(&val.to_string()).unwrap()
    }

    fn dec_to_f64(val: impl Display) -> f64 {
        f64::from_str(&val.to_string()).unwrap()
    }

    fn assert_values(dec: impl Display, f64_val: f64) {
        let dec_val = dec_to_f64(dec);
        if (dec_val - f64_val).abs() > 0.001f64 {
            assert_eq!(dec_val, f64_val)
        }
    }

    fn compute(x1: f64, x2: f64, a: f64, gamma: f64) -> AnyResult<()> {
        println!("{x1}, {x2}, a: {a}");
        let xp = [x1, x2];

        let x1_dec = f64_to_dec(x1);
        let x2_dec = f64_to_dec(x2);
        let xp_dec = [x1_dec, x2_dec];
        let a_dec = f64_to_dec(a);
        let gamma_dec = f64_to_dec(gamma);

        let d_f64 = newton_d_f64(&xp, a, gamma);
        let d_dec = newton_d(&xp_dec, a_dec, gamma_dec).unwrap();
        assert_values(d_dec, d_f64);

        let xp_swap = [0f64, x2 + 3.0];
        let y1_f64 = newton_y_f64(&xp_swap, a, gamma, d_f64, 0);
        let xp_swap_dec = [Decimal256::zero(), x2_dec + f64_to_dec(3.0)];
        if let Ok(res) = newton_y(&xp_swap_dec, a_dec, gamma_dec, d_dec, 0) {
            assert_values(res, y1_f64);
        } else {
            return Err(anyhow!("newton_y does not converge for i = 0"));
        }

        let y2_f64 = newton_y_f64(&[x1 + 1.0, 0f64], a, gamma, d_f64, 1);
        if let Ok(res) = newton_y(
            &[x1_dec + f64_to_dec(1.0), Decimal256::zero()],
            a_dec,
            gamma_dec,
            d_dec,
            1,
        ) {
            assert_values(res, y2_f64);
            Ok(())
        } else {
            Err(anyhow!("newton_y does not converge for i = 1"))
        }
    }

    #[test]
    fn single_test() {
        let gamma = 0.000145;

        compute(1000f64, 1000f64, 3500f64, gamma).unwrap();
    }

    #[test]
    fn test_real_case() {
        let x0 = 1173700.016159;
        let x1 = 0.800244312479334221;
        let offer_amount = 1.0;
        let amp = 40.0;
        let gamma = 0.000145;
        let d = 2064.855164704653967332;

        println!("Pool before [{} {}]", x0, x1);
        let new_x1 = newton_y(
            &[f64_to_dec(x0 + offer_amount), f64_to_dec(x1)],
            f64_to_dec(amp),
            f64_to_dec(gamma),
            f64_to_dec(d),
            1,
        )
        .unwrap();
        let new_x1 = dec_to_f64(new_x1);
        println!("Pool after [{} {}]", x0 + offer_amount, new_x1);
        println!("Diff [{} {}]", offer_amount, new_x1 - x1);
        assert!(new_x1 < x1, "new x1 {new_x1} should be less than x1 {x1}");
    }

    #[test]
    fn test_compute_d_for_lsd() {
        let pools = &[
            Decimal256::from_atomics(888787u128, 6).unwrap(),
            Decimal256::from_atomics(868901000520175167u128, 18).unwrap(),
        ];
        let amp = Decimal256::from_atomics(500u16, 0).unwrap();
        let gamma = Decimal256::from_atomics(1u8, 8).unwrap();

        let d = newton_d(pools, amp, gamma).unwrap();

        assert_eq!(d.to_string(), "1.757575508957576976")
    }

    #[test]
    fn test_compute_d_roar() {
        let pools = &[
            Decimal256::from_atomics(12089244654_099852u128, 6).unwrap(),
            Decimal256::from_atomics(23173729615_834822866013755721u128, 18).unwrap(),
        ];

        // 10.0
        let amp = Decimal256::from_atomics(10u8, 0).unwrap();
        // 0.000145
        let gamma = Decimal256::from_atomics(145u8, 6).unwrap();
        let d = newton_d(pools, amp, gamma).unwrap();
        assert_eq!(d.to_string(), "33532826223.999399077170285763")
    }

    #[test]
    fn test_derivatives() {
        let a_f64 = 3500f64;
        let gamma_f64 = 0.000145;
        let d_f64 = 2000000f64;
        let (x1, x2) = (1_000000f64, 1_000000f64);

        let a = f64_to_dec(a_f64);
        let gamma = f64_to_dec(gamma_f64);
        let d = f64_to_dec(d_f64);
        let x: [SignedDecimal256; 2] = [f64_to_dec(x1).into(), f64_to_dec(x2).into()];

        let der_f64 = crate::math::math_f64::df_dd(d_f64, &[x1, x2], a_f64, gamma_f64);
        let der = df_dd(d.into(), &x, a, gamma);
        assert_values(der, der_f64);

        let dx_f64 = crate::math::math_f64::df_dx(d_f64, &[x1, x2], a_f64, gamma_f64, 0);
        let dx = df_dx(d, &x, a, gamma, 0);
        assert_values(dx, dx_f64);
    }

    #[test]
    fn test_f() {
        let a = f64_to_dec(40f64);
        let gamma = f64_to_dec(0.000145);
        let d = f64_to_dec(20000000f64);
        let x: [SignedDecimal256; 2] = [
            f64_to_dec(1000000f64).into(),
            f64_to_dec(100000000f64).into(),
        ];

        let val = f(d.into(), &x, a, gamma);
        let val_f64 =
            crate::math::math_f64::f(20000000f64, &[1000000f64, 100000000f64], 40f64, 0.000145);
        let dec_val_f64 = dec_to_f64(val);
        assert!(
            (dec_val_f64 - val_f64).abs() > 1e-3,
            "Assert failed: {dec_val_f64} !~ {val_f64}"
        )
    }

    #[ignore]
    #[test]
    fn test_calculations() {
        let gamma = 0.000145;

        let x_range = (1000u128..=100_000)
            .step_by(10000)
            .into_iter()
            .collect_vec();
        let mut a_range = (100u128..=10000u128).step_by(1000).collect_vec();
        a_range.push(1);

        for (&x1, &x2) in x_range.iter().cartesian_product(&x_range) {
            for a in &a_range {
                compute(x1 as f64, x2 as f64, *a as f64, gamma).unwrap();
            }
        }
    }

    #[test]
    fn test_halfpow() {
        let res = half_float_pow(f64_to_dec(3.231f64)).unwrap();
        assert_eq!(dec_to_f64(res), 0.10650551189033386);

        let res = half_float_pow(f64_to_dec(0.5012f64)).unwrap();
        assert_eq!(dec_to_f64(res), 0.7065188709002241);

        let res = half_float_pow(f64_to_dec(59.1f64)).unwrap();
        assert_eq!(dec_to_f64(res), 0f64);
    }
}
