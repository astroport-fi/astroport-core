use astroport::asset::{Asset, AssetInfo};
use cosmwasm_std::{StdError, StdResult, Uint128, Uint256, Uint64};
use itertools::Itertools;

/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 32;

pub const MAX_AMP: u64 = 1_000_000;
pub const MAX_AMP_CHANGE: u64 = 10;
pub const MIN_AMP_CHANGING_TIME: u64 = 86400;
pub const AMP_PRECISION: u64 = 100;

/// ## Description
/// Calculates the ask amount (the amount of tokens swapped to).
/// ## Params
/// * **offer_pool** is an object of type [`Uint128`]. This is the amount of offer tokens currently in a stableswap pool.
///
/// * **ask_pool** is an object of type [`Uint128`]. This is the amount of ask tokens currently in a stableswap pool.
///
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of offer tokens to swap.
///
/// * **amp** is an object of type [`Uint64`]. This is the pool's amplification parameter.
pub fn calc_ask_amount(
    _offer_pool: Uint128,
    _ask_pool: Uint128,
    _offer_amount: Uint128,
    _amp: Uint64,
) -> StdResult<Uint128> {
    // TODO: update this function during cumulative prices redesign
    // let leverage = amp.checked_mul(N_COINS.into())?;
    // let new_offer_pool = offer_pool.checked_add(offer_amount)?;
    //
    // let d = compute_d(leverage, offer_pool, ask_pool)?;
    //
    // let new_ask_pool = compute_new_balance(leverage, new_offer_pool, d)?;
    //
    // let amount_swapped = ask_pool.checked_sub(new_ask_pool)?;
    // Ok(amount_swapped)

    Ok(Uint128::zero())
}

/// ## Description
/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// ## Params
/// * **amp** is an object of type [`Uint64`].
///
/// * **pools** is a vector with values of type [`Uint128`].
pub(crate) fn compute_d(amp: Uint64, pools: &[Uint128]) -> StdResult<Uint128> {
    let sum_x = pools
        .iter()
        .fold(Uint256::zero(), |acc, x| acc + (Uint256::from(*x)));

    if sum_x.is_zero() {
        Ok(Uint128::zero())
    } else {
        let n_coins = pools.len() as u8;
        let ann: Uint256 = (amp.checked_mul(n_coins.into())?.u64() / AMP_PRECISION).into();
        let n_coins = Uint256::from(n_coins);
        let mut d = sum_x;
        let ann_sum_x = ann * sum_x;
        for _ in 0..ITERATIONS {
            // loop: D_P = D_P * D / (_x * N_COINS + 1)
            let d_p = pools
                .iter()
                .try_fold::<_, _, StdResult<_>>(d, |acc, pool| {
                    let denominator =
                        Uint256::from(*pool).checked_mul(n_coins)? + Uint256::from(1u8);
                    Ok(acc
                        .checked_multiply_ratio(d, denominator)
                        .map_err(|_| StdError::generic_err("CheckedMultiplyRatioError"))?)
                })?;
            let d_prev = d;
            d = (ann_sum_x + d_p * n_coins) * d
                / ((ann - Uint256::from(1u8)) * d + (n_coins + Uint256::from(1u8)) * d_p);
            if d >= d_prev {
                if d - d_prev <= Uint256::from(1u8) {
                    return Ok(d.try_into()?);
                }
            } else if d < d_prev {
                if d_prev - d <= Uint256::from(1u8) {
                    return Ok(d.try_into()?);
                }
            }
        }

        Ok(d.try_into()?)
    }
}

/// ## Description
/// Computes the new balance of a `to` pool if one makes `from` pool = `new_amount`.  
///
/// Done by solving quadratic equation iteratively.  
///
/// `x_1**2 + x_1 * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n + 1) / (n ** (2 * n) * prod' * A)`  
///
/// `x_1**2 + b*x_1 = c`  
///
/// `x_1 = (x_1**2 + c) / (2*x_1 + b)`
pub(crate) fn calc_y(
    from: &AssetInfo,
    to: &AssetInfo,
    new_amount: Uint128,
    pools: &[Asset],
    amp: Uint64,
) -> StdResult<Uint128> {
    let n_coins = pools.len() as u8;
    let ann: Uint256 = (amp.checked_mul(n_coins.into())?.u64() / AMP_PRECISION).into();
    let n_coins = Uint256::from(n_coins);

    let mut sum = Uint256::zero();
    let pool_values = pools.iter().map(|asset| asset.amount).collect_vec();
    let d: Uint256 = compute_d(amp, &pool_values)?.into();
    let mut c = d;
    for pool in pools {
        let pool_amount: Uint256 = if pool.info.eq(from) {
            new_amount
        } else if !pool.info.eq(to) {
            pool.amount
        } else {
            continue;
        }
        .into();
        sum += pool_amount;
        c = c * d / (pool_amount * n_coins)
    }
    c = c * d / (ann * n_coins);
    let b = sum + d / ann;
    let mut y = d;
    for _ in 0..ITERATIONS {
        let y_prev = y;
        y = (y * y + c) / (y + y + b - d);
        if y >= y_prev {
            if y - y_prev <= Uint256::from(1u8) {
                return Ok(y.try_into()?);
            }
        } else if y < y_prev {
            if y_prev - y <= Uint256::from(1u8) {
                return Ok(y.try_into()?);
            }
        }
    }

    // Should definitely converge in 32 iterations.
    Err(StdError::generic_err("y is not converging"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport::asset::native_asset;
    use sim::StableSwapModel;

    #[test]
    fn test_compute_d() {
        let amp = Uint64::from(100u64);
        let pool1 = Uint128::from(100_000u128);
        let pool2 = Uint128::from(100_000u128);
        let pool3 = Uint128::from(100_000u128);
        let model = StableSwapModel::new(
            amp.u64().into(),
            vec![pool1.u128(), pool2.u128(), pool3.u128()],
            3,
        );

        let sim_d = model.sim_d();
        let d = compute_d(amp, &vec![pool1, pool2, pool3]).unwrap().u128();

        assert_eq!(sim_d, d);
    }

    #[test]
    fn test_compute_y() {
        let amp = Uint64::from(100u64);
        let pool1 = Uint128::from(100_000_000000u128);
        let pool2 = Uint128::from(100_000_000000u128);
        let pool3 = Uint128::from(100_000_000000u128);
        let model = StableSwapModel::new(
            amp.u64().into(),
            vec![pool1.u128(), pool2.u128(), pool3.u128()],
            3,
        );

        let pools = vec![
            native_asset("test1".to_string(), pool1),
            native_asset("test2".to_string(), pool2),
            native_asset("test3".to_string(), pool3),
        ];

        let offer_amount = Uint128::from(100_000000u128);
        let sim_y = model.sim_y(0, 1, pool2.u128() + offer_amount.u128());
        let y = calc_y(
            &pools[0].info,
            &pools[1].info,
            pools[0].amount + offer_amount,
            &pools,
            amp * Uint64::from(AMP_PRECISION),
        )
        .unwrap()
        .u128();

        assert_eq!(sim_y, y);
    }
}
