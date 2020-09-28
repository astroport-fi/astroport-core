use cosmwasm_std::{Decimal, StdResult, Uint128};

/////////////////////////////////////////////////////////////
const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000u128);

pub fn reverse_decimal(decimal: Decimal) -> Decimal {
    if decimal.is_zero() {
        return Decimal::zero();
    }

    Decimal::from_ratio(DECIMAL_FRACTIONAL, decimal * DECIMAL_FRACTIONAL)
}

pub fn decimal_subtraction(a: Decimal, b: Decimal) -> StdResult<Decimal> {
    Ok(Decimal::from_ratio(
        (a * DECIMAL_FRACTIONAL - b * DECIMAL_FRACTIONAL)?,
        DECIMAL_FRACTIONAL,
    ))
}

pub fn decimal_multiplication(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(a * DECIMAL_FRACTIONAL * b, DECIMAL_FRACTIONAL)
}
