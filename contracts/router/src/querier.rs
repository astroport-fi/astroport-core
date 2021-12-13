use cosmwasm_std::{Decimal, Deps, StdResult, Uint128};
use terra_cosmwasm::TerraQuerier;

/// ## Description
/// Sets the default decimal fraction
static DECIMAL_FRACTION: Uint128 = Uint128::new(1_000_000_000_000_000_000u128);

/// ## Description
/// If **denom* is a **uluna** returns zero, otherwise calculates and returns compute tax.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **amount** is the object of type [`Uint128`].
///
/// * **denom** is the object of type [`String`].
pub fn compute_tax(deps: Deps, amount: Uint128, denom: String) -> StdResult<Uint128> {
    if denom == "uluna" {
        return Ok(Uint128::zero());
    }

    let terra_querier = TerraQuerier::new(&deps.querier);
    let tax_rate: Decimal = (terra_querier.query_tax_rate()?).rate;
    let tax_cap: Uint128 = (terra_querier.query_tax_cap(denom)?).cap;
    Ok(std::cmp::min(
        amount.checked_sub(amount.multiply_ratio(
            DECIMAL_FRACTION,
            DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
        ))?,
        tax_cap,
    ))
}
