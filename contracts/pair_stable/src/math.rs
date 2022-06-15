use cosmwasm_std::{StdError, StdResult, Uint128, Uint256, Uint64};

const N_COINS_SQUARED: u8 = 4;
const ITERATIONS: u8 = 32;

pub const N_COINS: u8 = 2;
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
    offer_pool: Uint128,
    ask_pool: Uint128,
    offer_amount: Uint128,
    amp: Uint64,
) -> StdResult<Uint128> {
    // TODO: fix calc ask amount
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
/// Calculates the amount to be swapped (the offer amount).
/// ## Params
/// * **offer_pool** is an object of type [`Uint128`]. This is the amount of offer tokens currently in a stableswap pool.
///
/// * **ask_pool** is an object of type [`Uint128`]. This is the amount of ask tokens currently in a stableswap pool.
///
/// * **ask_amount** is an object of type [`Uint128`]. This is the amount of offer tokens to swap.
///
/// * **amp** is an object of type [`Uint64`]. This is the pool's amplification parameter.
pub fn calc_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
    amp: Uint64,
) -> StdResult<Uint128> {
    // TODO: fix calc offer amount
    // let leverage = amp.checked_mul(N_COINS.into())?;
    // let new_ask_pool = ask_pool.checked_sub(ask_amount)?;
    //
    // let d = compute_d(leverage, offer_pool, ask_pool)?;
    //
    // let new_offer_pool = compute_new_balance(leverage, new_ask_pool, d)?;
    //
    // Ok(new_offer_pool.checked_sub(offer_pool)?)

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
/// * **n_coins** is a number of coins in the pool.
///
/// * **leverage** is an object of type [`Uint64`].
///
/// * **pools** is a vector with values of type [`Uint128`].
pub fn compute_d(n_coins: u8, leverage: Uint64, pools: &[Uint128]) -> StdResult<Uint128> {
    let sum_x = pools
        .iter()
        .try_fold(Uint256::zero(), |acc, x| acc.checked_add(Uint256::from(*x)))?;

    if sum_x.is_zero() {
        Ok(Uint128::zero())
    } else {
        let n_coins: Uint256 = n_coins.into();
        let mut d = sum_x;
        let ann = Uint256::from(leverage).checked_mul(n_coins)?;
        let ann_sum_x = ann.checked_mul(sum_x)?;
        for _ in 0..ITERATIONS {
            let d_p = pools
                .iter()
                .try_fold::<_, _, StdResult<_>>(d, |acc, pool| {
                    let denominator = Uint256::from(*pool).checked_mul(n_coins)?;
                    Ok(acc.checked_mul(d)?.checked_div(denominator)?)
                })?;
            let d_prev = d;
            d = (ann_sum_x + d_p * n_coins) * d / (ann * d + (n_coins + Uint256::from(1u8)) * d_p);
            if d > d_prev {
                if d - d_prev <= Uint256::from(1u8) {
                    return Ok(d.try_into()?);
                }
            } else if d_prev - d <= Uint256::from(1u8) {
                return Ok(d.try_into()?);
            }
        }

        // Should definitely converge in 32 iterations.
        Err(StdError::generic_err("D is not converging"))
    }
}

/// ## Description
/// Compute the swap amount `y` in proportion to `x`.
///
/// * **Solve for y**
///
/// y**2 + y * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n + 1) / (n ** (2 * n) * prod' * A)
///
/// y**2 + b*y = c
fn compute_new_balance(
    leverage: Uint64,
    new_source_amount: Uint128,
    d_val: Uint128,
) -> StdResult<Uint128> {
    let leverage: Uint256 = leverage.into();
    let new_source_amount: Uint256 = new_source_amount.into();
    let d_val: Uint256 = d_val.into();

    // sum' = prod' = x
    // c =  D ** (n + 1) / (n ** (2 * n) * prod' * A)
    let numerator = d_val
        .checked_pow((N_COINS + 1) as u32)?
        .checked_mul(AMP_PRECISION.into())?;
    let denominator = new_source_amount
        .checked_mul(N_COINS_SQUARED.into())?
        .checked_mul(leverage)?;
    let c = numerator.checked_div(denominator)?;

    // b = sum' - (A*n**n - 1) * D / (A * n**n)
    let b = new_source_amount.checked_add(
        d_val
            .checked_mul(AMP_PRECISION.into())?
            .checked_div(leverage)?,
    )?;

    // Solve for y by approximating: y**2 + b*y = c
    let mut y_prev;
    let mut y = d_val;
    for _ in 0..ITERATIONS {
        y_prev = y;
        let numerator = y.checked_pow(2u32)?.checked_add(c)?;
        let denominator = y
            .checked_mul(2u8.into())?
            .checked_add(b)?
            .checked_sub(d_val)?;
        y = numerator.checked_div(denominator)?;
        if y == y_prev {
            break;
        }
    }

    Ok(y.try_into()?)
}
