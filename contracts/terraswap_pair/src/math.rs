use cosmwasm_std::{Decimal, StdResult, Uint128};
use fixed::types::I64F64;
use fixed::transcendental::pow as fixed_pow;
use std::ops::{Div, Mul, Add, Sub};

pub type FixedFloat = I64F64;

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

pub fn calc_out_given_in(
    balance_in: Uint128,
    weight_in: FixedFloat,
    balance_out: Uint128,
    weight_out: FixedFloat,
    amount_in: Uint128,
) -> Uint128 {
    let adjusted_in = balance_in.add(amount_in);

    let y = FixedFloat::from_num(balance_in.u128() * DECIMAL_FRACTIONAL.u128() / adjusted_in.u128());
    let y = y.div(&FixedFloat::from_num(DECIMAL_FRACTIONAL.u128()));

    let weight_ratio = weight_in.div(&weight_out);

    let multiplier: FixedFloat = fixed_pow(y, weight_ratio).unwrap();
    let multiplier = FixedFloat::from_num(1).sub(multiplier);

    let amount_out: u128 = FixedFloat::from_num(balance_out.u128()).mul(&multiplier).to_num();

    Uint128(amount_out)
}

pub fn calc_in_given_out(
    balance_in: Uint128,
    weight_in: FixedFloat,
    balance_out: Uint128,
    weight_out: FixedFloat,
    amount_out: Uint128,
) -> Uint128 {
    let updated_balance = balance_out.sub(amount_out).unwrap();

    let weight_ratio = weight_out.div(&weight_in);

    let y = FixedFloat::from_num(balance_out.u128() * DECIMAL_FRACTIONAL.u128() / updated_balance.u128());
    let y = y.div(&FixedFloat::from_num(DECIMAL_FRACTIONAL.u128()));

    let multiplier: FixedFloat = fixed_pow(y, weight_ratio).unwrap();
    let multiplier = multiplier.sub(FixedFloat::from_num(1));

    let amount_in: u128 = FixedFloat::from_num(balance_in.u128()).mul(&multiplier).to_num();

    Uint128(amount_in)
}
