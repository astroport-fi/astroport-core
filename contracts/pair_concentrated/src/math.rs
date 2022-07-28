use cosmwasm_std::{Env, Isqrt, StdError, StdResult, Uint128, Uint256};

use astroport::cosmwasm_ext::{AbsDiff, OneValue};

use crate::constants::{
    A_MULTIPLIER, EXP_PRECISION, ITERATIONS, MULTIPLIER, N_COINS, PRECISION, UINT256_E14,
};
use crate::state::{PoolParams, PoolState};

pub(crate) fn geometric_mean(values: &[Uint256]) -> Uint256 {
    let mul = values[0] * values[1];
    mul.isqrt()
}

pub(crate) fn newton_d(ann: Uint256, gamma: Uint256, pools: &[Uint256]) -> StdResult<Uint256> {
    let x = if pools[0] < pools[1] {
        vec![pools[1], pools[0]]
    } else {
        pools.to_vec()
    };

    let mut d = N_COINS * geometric_mean(&x);
    let sum: Uint256 = x.iter().sum();
    let init_g1k0 = gamma + MULTIPLIER;

    for _ in 0..ITERATIONS {
        let d_prev = d;

        let k0 = MULTIPLIER * N_COINS * N_COINS * x[0] / d * x[1] / d;

        let g1k0 = init_g1k0.diff(k0) + Uint256::one();

        let mul1 = MULTIPLIER * d / gamma * g1k0 / gamma * g1k0 * A_MULTIPLIER / ann;
        let mul2 = (Uint256::from(2u8) * MULTIPLIER * N_COINS) * k0 / g1k0;

        let neg_fprime =
            sum + sum * mul2 / MULTIPLIER + mul1 * N_COINS / k0 - mul2 * d / MULTIPLIER;

        let d_plus = d * (neg_fprime + sum) / neg_fprime;
        let mut d_minus = d * d / neg_fprime;

        if MULTIPLIER > k0 {
            d_minus += d * (mul1 / neg_fprime) / MULTIPLIER * (MULTIPLIER - k0) / k0
        } else {
            d_minus -= d * (mul1 / neg_fprime) / MULTIPLIER * (k0 - MULTIPLIER) / k0
        }

        if d_plus > d_minus {
            d = d_plus - d_minus
        } else {
            d = (d_minus - d_plus) / Uint256::from(2u8)
        }

        let d_diff = d.diff(d_prev);
        if d_diff * UINT256_E14 < Uint256::from_u128(1e16 as u128).max(d) {
            return Ok(d);
        }
    }

    Err(StdError::generic_err("newton_d is not converging"))
}

pub(crate) fn newton_y(
    ann: Uint256, // A * n^n * A_MULTIPLIER
    gamma: Uint256,
    pools: &[Uint256],
    d: Uint256,
    i: usize, // an index of pool which balance is unknown
) -> StdResult<Uint256> {
    let pool_j = pools[1 - i]; // Other pool which balance is known
    let mut y = d * d / (pool_j * N_COINS * N_COINS);
    let k0_i = (MULTIPLIER * N_COINS) * pool_j / d;

    let convergence_limit = (pool_j / UINT256_E14)
        .max(d / UINT256_E14)
        .max(Uint256::from(100u8));

    let init_g1k0 = gamma + MULTIPLIER;

    for _ in 0..ITERATIONS {
        let y_prev = y;

        let k0 = (k0_i * y * N_COINS).checked_div(d)?;
        let s = pool_j + y;

        let g1k0 = init_g1k0.diff(k0) + Uint256::one();

        let mul1 = (MULTIPLIER * d / gamma * g1k0 / gamma * g1k0 * A_MULTIPLIER) / ann;
        let mul2 = (MULTIPLIER + Uint256::from(2u8) * MULTIPLIER * k0) / g1k0;

        let mut yfprime = MULTIPLIER * y + s * mul2 + mul1;
        let dyfprime = d * mul2;

        if yfprime < dyfprime {
            y = y_prev / Uint256::from(2u8);
            continue;
        } else {
            yfprime -= dyfprime
        }
        let fprime = yfprime / y;

        let mut y_minus = mul1 / fprime;
        let y_plus = (yfprime + MULTIPLIER * d) / fprime + y_minus * MULTIPLIER / k0;
        y_minus += MULTIPLIER * s / fprime;

        if y_plus < y_minus {
            y = y_prev / Uint256::from(2u8);
        } else {
            y = y_plus - y_minus
        }

        let diff = y_prev.diff(y);

        if diff < convergence_limit.max(y / UINT256_E14) {
            return Ok(y);
        }
    }

    Err(StdError::generic_err("newton_y is not converging"))
}

pub(crate) fn halfpow(power: Uint256) -> StdResult<Uint256> {
    let intpow = power / MULTIPLIER;
    let intpow_u128: Uint128 = intpow.try_into()?;
    if intpow_u128.u128() > 59u128 {
        return Ok(Uint256::zero());
    }

    let frac_pow = power - intpow * MULTIPLIER;
    let result = MULTIPLIER / Uint256::from(2u8).pow(intpow_u128.u128() as u32);

    if frac_pow.is_zero() {
        return Ok(result);
    }

    let mut term = MULTIPLIER;
    let mut s = MULTIPLIER;
    let mut neg = false;

    for i in 1..ITERATIONS {
        let k = Uint256::from(i as u128) * MULTIPLIER;
        let mut c = k - MULTIPLIER;
        if frac_pow > c {
            neg = !neg;
        }
        c = frac_pow.diff(c);
        term = term * c / Uint256::from(2u8) / k;
        if neg {
            s -= term;
        } else {
            s += term;
        }
        if term < EXP_PRECISION {
            return Ok(result * s / MULTIPLIER);
        }
    }

    Err(StdError::generic_err("halfpow is not converging"))
}

pub(crate) fn update_price(
    pool_state: &mut PoolState,
    env: &Env,
    init_xp: Vec<Uint256>,
    new_price: Uint256,
    new_d: Uint256,
    pool_params: &PoolParams,
    total_lp: Uint256,
) -> StdResult<()> {
    let mut price_state = pool_state.price_state;
    let block_time = env.block.time.seconds();
    if price_state.last_price_update < block_time {
        let arg = Uint256::from(block_time - price_state.last_price_update) * MULTIPLIER
            / Uint256::from(pool_params.ma_half_time);
        let alpha = halfpow(arg)?;
        price_state.price_oracle = (price_state.last_prices * (MULTIPLIER - alpha)
            + price_state.price_oracle * alpha)
            / MULTIPLIER;
        price_state.last_price_update = block_time;
    }

    let a_gamma = pool_state.get_amp_gamma(env);
    let (ann, gamma) = (a_gamma.ann(), a_gamma.gamma());
    let d_unadjusted = if new_d.is_zero() {
        newton_d(ann, gamma, &init_xp)?
    } else {
        new_d
    };

    price_state.last_prices = if !new_price.is_zero() {
        new_price
    } else {
        let mut tmp_xp = init_xp.clone();
        let dx_price = tmp_xp[0] / Uint256::from(1000000u32);
        tmp_xp[0] += dx_price;
        price_state.price_scale * dx_price
            / (tmp_xp[1] - newton_y(ann, gamma, &tmp_xp, d_unadjusted, 1)?)
    };

    let xp = [
        d_unadjusted / N_COINS,
        d_unadjusted * PRECISION / (N_COINS * price_state.price_scale),
    ];
    let mut xcp_profit = MULTIPLIER;
    let mut virtual_price = MULTIPLIER;

    if !price_state.virtual_price.is_zero() {
        let xcp = geometric_mean(&xp);
        virtual_price = MULTIPLIER * xcp / total_lp;
        xcp_profit = price_state.xcp_profit * virtual_price / price_state.virtual_price;

        if virtual_price < price_state.virtual_price && pool_state.future_time == 0 {
            return Err(StdError::generic_err("Loss"));
        }
        // TODO: why?
        if pool_state.future_time == 1 {
            pool_state.future_time = 0;
        }
    }

    price_state.xcp_profit = xcp_profit;

    let norm = (price_state.price_oracle * MULTIPLIER / price_state.price_scale).diff(MULTIPLIER);
    let adjustment_step = (norm / Uint256::from(10u8)).max(pool_params.adjustment_step.into());

    let mut need_adjustment = price_state.not_adjusted;
    if !need_adjustment
        && virtual_price * Uint256::from(2u8) - MULTIPLIER
            > xcp_profit + pool_params.allowed_extra_profit * Uint256::from(2u8)
        && norm > adjustment_step
        && !price_state.virtual_price.is_zero()
    {
        price_state.not_adjusted = true;
        need_adjustment = true;
    }

    if need_adjustment && norm > adjustment_step && !price_state.virtual_price.is_zero() {
        let numerator = price_state.price_scale * (norm - adjustment_step)
            + adjustment_step * price_state.price_oracle;
        let price_scale_new = numerator / norm;
        let xp = [
            init_xp[0],
            init_xp[1] * price_scale_new / price_state.price_scale,
        ];
        let d = newton_d(ann, gamma, &xp)?;

        let xp = [d / N_COINS, d * PRECISION / (N_COINS * price_scale_new)];
        let old_virtual_price = MULTIPLIER * geometric_mean(&xp) / total_lp;
        if old_virtual_price > MULTIPLIER
            && Uint256::from(2u8) * old_virtual_price - MULTIPLIER > xcp_profit
        {
            price_state.price_scale = price_scale_new;
            price_state.d = d;
            price_state.virtual_price = old_virtual_price;
        } else {
            price_state.not_adjusted = false;
            price_state.d = d_unadjusted;
            price_state.virtual_price = virtual_price;
        }

        pool_state.price_state = price_state;
        return Ok(());
    }

    price_state.d = d_unadjusted;
    price_state.virtual_price = virtual_price;

    if need_adjustment {
        price_state.not_adjusted = false;
    }
    pool_state.price_state = price_state;

    Ok(())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Timestamp;
    use sim::model::{Caller, ConcentratedPairModel, A_MUL, MUL_E18};

    use crate::state::{AmpGamma, PriceState};

    use super::*;

    #[test]
    fn check_geometric_mean() {
        let values = [1000u128 * MUL_E18, 5000u128 * MUL_E18];

        let uint256_values: Vec<Uint256> = values.iter().cloned().map(Uint256::from_u128).collect();
        let result = geometric_mean(&uint256_values);
        assert_eq!(result / MULTIPLIER, Uint256::from(2236u32));

        let model_result: u128 = Caller::new()
            .call_func("geometric_mean", (values,))
            .unwrap();
        let u128_result: Uint128 = result.try_into().unwrap();
        assert_eq!(model_result / MUL_E18, u128_result.u128() / MUL_E18);
    }

    #[test]
    fn check_halfpow() {
        let mul_float = MUL_E18 as f64;
        let power = 0.49833333333333333;
        let power_u256 = Uint256::from((power * mul_float) as u128);

        let real_result = 0.5f64.powf(power);
        let result: Uint128 = halfpow(power_u256).unwrap().try_into().unwrap();

        let tolerance = Uint128::try_from(EXP_PRECISION).unwrap().u128() as f64 / mul_float;
        if (real_result - result.u128() as f64 / mul_float).abs() > tolerance {
            assert_eq!(real_result, result.u128() as f64 / mul_float);
        }
    }

    #[test]
    fn check_compute_d() {
        let amp = 100 * A_MUL;
        let gamma = (1e-4 * MUL_E18 as f64) as u128;
        let pools = vec![1_000_000 * MUL_E18, 500_000 * MUL_E18];
        let prices = vec![MUL_E18, 2 * MUL_E18];
        let uint256_pools: Vec<Uint256> = pools
            .iter()
            .zip(&prices)
            .map(|(amount, price)| Uint256::from_u128(amount * (price / MUL_E18)))
            .collect();

        // ann = A * N^N which is A * 2^2 = A * 4
        let model = ConcentratedPairModel::new_default(amp * 4, gamma, pools, 2, prices).unwrap();
        let model_result: u128 = model.call_curve("D", ()).unwrap();

        let result =
            newton_d(Uint256::from(amp * 4), Uint256::from(gamma), &uint256_pools).unwrap();

        assert_eq!(result / MULTIPLIER, Uint256::from(2_000_000u128));
        let u128_result: Uint128 = result.try_into().unwrap();
        assert_eq!(u128_result.u128(), model_result);
    }

    #[test]
    fn check_compute_d_second() {
        let amp = 100 * A_MUL;
        let gamma = (0.000145 * MUL_E18 as f64) as u128;
        let pools = vec![98181568049804, 101844718939576];
        let prices = vec![MUL_E18, MUL_E18];
        let uint256_pools: Vec<Uint256> = pools
            .iter()
            .zip(&prices)
            .map(|(amount, price)| Uint256::from_u128(amount * (price / MUL_E18)))
            .collect();

        let model_result: u128 = Caller::new()
            .call_func("solve_D_vyper", (amp * 4, gamma, pools))
            .unwrap();

        let result =
            newton_d(Uint256::from(amp * 4), Uint256::from(gamma), &uint256_pools).unwrap();

        let u128_result: Uint128 = result.try_into().unwrap();
        assert_eq!(u128_result.u128(), model_result);
    }

    #[test]
    fn check_compute_y() {
        let amp = 100 * A_MUL;
        let gamma = (1e-4 * MUL_E18 as f64) as u128;
        let pools = vec![1_000_000 * MUL_E18, 1_000_000 * MUL_E18];
        let prices = vec![MUL_E18, MUL_E18]; // equal prices
        let mut uint256_pools: Vec<Uint256> = pools
            .iter()
            .map(|amount| Uint256::from_u128(*amount))
            .collect();

        let offer_amount = 500_000 * MUL_E18;

        let model =
            ConcentratedPairModel::new_default(amp * 4, gamma, pools.clone(), 2, prices).unwrap();
        let model_result: u128 = model
            .call_curve("y", (pools[0] + offer_amount, 0, 1))
            .unwrap();

        let d = newton_d(Uint256::from(amp * 4), Uint256::from(gamma), &uint256_pools).unwrap();
        uint256_pools[0] += Uint256::from(offer_amount);
        let result = newton_y(
            Uint256::from(amp * 4),
            Uint256::from(gamma),
            &uint256_pools,
            Uint256::from(d),
            1, // <- ask pool index
        )
        .unwrap();

        let res_u128: Uint128 = result.try_into().unwrap();
        // New value of ask pool (this is not return amount!)
        assert_eq!(model_result / MUL_E18, res_u128.u128() / MUL_E18);
    }

    #[test]
    fn check_update_price() {
        let amp = 100 * A_MUL;
        let gamma = (0.000145 * MUL_E18 as f64) as u128;
        let pools = vec![1_000_000 * MUL_E18, 1_000_000 * MUL_E18];
        let total_lp = Uint128::from(1_000_000u128); // total LP tokens amount
        let prices = vec![MUL_E18, MUL_E18]; // equal prices
        let mut xp: Vec<Uint256> = pools
            .iter()
            .map(|amount| Uint256::from_u128(*amount))
            .collect();
        let tolerance = Uint256::from_u128((1e-10 * 1e18) as u128); // allowed difference with Python model

        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(0);
        let mut pool_state = PoolState {
            initial: AmpGamma {
                amp: Default::default(),
                gamma: Default::default(),
            },
            future: AmpGamma {
                amp: amp.into(),
                gamma: gamma.into(),
            },
            future_time: env.block.time.seconds(),
            initial_time: env.block.time.seconds(),
            price_state: PriceState {
                price_oracle: MULTIPLIER,
                last_prices: MULTIPLIER,
                price_scale: MULTIPLIER,
                last_price_update: 0,
                xcp_profit: MULTIPLIER,
                virtual_price: Default::default(),
                d: Default::default(),
                not_adjusted: false,
            },
        };

        let params = PoolParams {
            mid_fee: Default::default(),
            out_fee: Default::default(),
            fee_gamma: Default::default(),
            allowed_extra_profit: Uint256::zero(),
            adjustment_step: Uint128::from((0.000146 * 1e18) as u128),
            ma_half_time: 600,
        };

        // Initialize python model
        let model = ConcentratedPairModel::new(
            amp * 4,
            gamma,
            pools.clone(),
            2,
            prices,
            0f64,
            0f64,
            0,
            params.adjustment_step.u128() as f64 / 1e18,
            params.ma_half_time,
        )
        .unwrap();

        // Sell huge amount of tokens thus making pool imbalanced
        let dy = 500_000u128 * MUL_E18;
        let dx: u128 = model.call("sell", (dy, 1, 0)).unwrap();
        let price: Uint128 = (Uint256::from(dy) * MULTIPLIER / Uint256::from(dx))
            .try_into()
            .unwrap();
        let _: u128 = model
            .call(
                "tweak_price",
                (env.block.time.seconds(), 0, 1, price.u128()),
            )
            .unwrap();

        // Simulating "selling"
        let amp_gamma = pool_state.get_amp_gamma(&env);
        let d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &xp).unwrap();
        xp[0] += Uint256::from(dy);
        let new_x = newton_y(amp_gamma.ann(), amp_gamma.gamma(), &xp, d, 1).unwrap();
        let dx = xp[1] - new_x;
        xp[1] = new_x;
        update_price(
            &mut pool_state,
            &env,
            xp.clone(),
            Uint256::from(dy) * MULTIPLIER / dx,
            Uint256::zero(),
            &params,
            total_lp.into(),
        )
        .unwrap();

        let model_result: Vec<u128> = model.get_attr("last_price").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.last_prices) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.last_prices,
                "last_price assertion failed"
            );
        }
        let model_result: Vec<u128> = model.get_attr("price_oracle").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.price_oracle) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.price_oracle,
                "price_oracle assertion failed"
            );
        }
        let model_result: Vec<u128> = model.get_attr_curve("p").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.price_scale) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.price_scale,
                "price_scale assertion failed"
            );
        }

        // Going to the future so EMA will be able to update internal oracle price
        env.block.time = env.block.time.plus_seconds(600);

        // Buyback those 500_000 tokens thus making profit for LPs
        let dx: u128 = model.call("sell", (dy, 0, 1)).unwrap();
        let price: Uint128 = (Uint256::from(dy) * MULTIPLIER / Uint256::from(dx))
            .try_into()
            .unwrap();
        let _: u128 = model
            .call(
                "tweak_price",
                (env.block.time.seconds(), 0, 1, price.u128()),
            )
            .unwrap();

        // Simulating "buying"
        let d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &xp).unwrap();
        xp[1] += Uint256::from(dy);
        let new_x = newton_y(amp_gamma.ann(), amp_gamma.gamma(), &xp, d, 0).unwrap();
        let dx = xp[0] - new_x;
        xp[0] = new_x;
        update_price(
            &mut pool_state,
            &env,
            xp.clone(),
            Uint256::from(dy) * MULTIPLIER / dx,
            Uint256::zero(),
            &params,
            total_lp.into(),
        )
        .unwrap();

        let model_result: Vec<u128> = model.get_attr("last_price").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.last_prices) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.last_prices,
                "last_price assertion failed"
            );
        }
        let model_result: Vec<u128> = model.get_attr("price_oracle").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.price_oracle) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.price_oracle,
                "price_oracle assertion failed"
            );
        }
        let model_result: Vec<u128> = model.get_attr_curve("p").unwrap();
        if Uint256::from(model_result[1]).diff(pool_state.price_state.price_scale) > tolerance {
            assert_eq!(
                Uint256::from(model_result[1]),
                pool_state.price_state.price_scale,
                "price_scale assertion failed"
            );
        }
    }
}
