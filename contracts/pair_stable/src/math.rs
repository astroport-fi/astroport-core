use cosmwasm_std::{StdResult, Uint128, Uint256, Uint64};

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
    let leverage = amp.checked_mul(N_COINS.into())?;
    let new_offer_pool = offer_pool.checked_add(offer_amount)?;

    let d = compute_d(leverage, offer_pool, ask_pool)?;

    let new_ask_pool = compute_new_balance(leverage, new_offer_pool, d)?;

    let amount_swapped = ask_pool.checked_sub(new_ask_pool)?;
    Ok(amount_swapped)
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
    let leverage = amp.checked_mul(N_COINS.into())?;
    let new_ask_pool = ask_pool.checked_sub(ask_amount)?;

    let d = compute_d(leverage, offer_pool, ask_pool)?;

    let new_offer_pool = compute_new_balance(leverage, new_ask_pool, d)?;

    Ok(new_offer_pool.checked_sub(offer_pool)?)
}

/// ## Description
/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// ## Params
/// * **leverage** is an object of type [`Uint64`].
///
/// * **amount_a** is an object of type [`Uint128`].
///
/// * **amount_b** is an object of type [`Uint128`].
pub fn compute_d(leverage: Uint64, amount_a: Uint128, amount_b: Uint128) -> StdResult<Uint128> {
    let n_coins = Uint128::from(N_COINS);
    let amount_a_times_coins = amount_a.full_mul(n_coins) + Uint256::from(1u8);
    let amount_b_times_coins = amount_b.full_mul(n_coins) + Uint256::from(1u8);
    let sum_x = amount_a.checked_add(amount_b)?; // sum(x_i), a.k.a S
    if sum_x.is_zero() {
        Ok(Uint128::zero())
    } else {
        let mut d_previous;
        let mut d: Uint256 = sum_x.into();

        // Newton's method to approximate D
        for _ in 0..ITERATIONS {
            let mut d_product = d;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_a_times_coins)?;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_b_times_coins)?;
            d_previous = d;
            // d = (leverage * sum_x + d_p * n_coins) * d / ((leverage - 1) * d + (n_coins + 1) * d_p);
            d = calculate_step(d, leverage, sum_x, d_product)?.into();
            // Equality with the precision of 1
            if d == d_previous {
                break;
            }
        }

        Ok(d.try_into()?)
    }
}

/// ## Description
/// Helper function used to calculate the D invariant as a last step in the `compute_d` public function.
///
/// * **Equation**:
///
/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
fn calculate_step(
    initial_d: Uint256,
    leverage: Uint64,
    sum_x: Uint128,
    d_product: Uint256,
) -> StdResult<Uint128> {
    let leverage_mul =
        Uint256::from(leverage).checked_mul(sum_x.into())? / Uint256::from(AMP_PRECISION);
    let d_p_mul = d_product.checked_mul(N_COINS.into())?;

    let l_val = leverage_mul.checked_add(d_p_mul)?.checked_mul(initial_d)?;

    let leverage_sub = initial_d
        .checked_mul((leverage.checked_sub(AMP_PRECISION.into())?).into())?
        / Uint256::from(AMP_PRECISION);
    let n_coins_sum = d_product.checked_mul((N_COINS + 1).into())?;

    let r_val = leverage_sub.checked_add(n_coins_sum)?;

    Ok(l_val.checked_div(r_val)?.try_into()?)
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
