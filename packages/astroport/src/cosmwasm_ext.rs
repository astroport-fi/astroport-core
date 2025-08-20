use std::ops;

use cosmwasm_std::{Decimal, Decimal256, Fraction, StdError, StdResult, Uint128, Uint256, Uint64};

pub trait AbsDiff
where
    Self: Copy + PartialOrd + ops::Sub<Output = Self>,
{
    fn diff(self, rhs: Self) -> Self {
        if self > rhs {
            self - rhs
        } else {
            rhs - self
        }
    }
}

impl AbsDiff for Uint256 {}
impl AbsDiff for Uint128 {}
impl AbsDiff for Uint64 {}
impl AbsDiff for Decimal {}
impl AbsDiff for Decimal256 {}

pub trait IntegerToDecimal
where
    Self: Copy + Into<Uint128> + Into<Uint256>,
{
    fn to_decimal(self) -> Decimal {
        Decimal::from_ratio(self, 1u8)
    }

    fn to_decimal256(self, precision: impl Into<u32>) -> StdResult<Decimal256> {
        Decimal256::from_atomics(self, precision.into())
            .map_err(|_| StdError::generic_err("Decimal256 range exceeded"))
    }
}

impl IntegerToDecimal for u8 {}
impl IntegerToDecimal for u64 {}
impl IntegerToDecimal for u128 {}
impl IntegerToDecimal for Uint128 {}

pub trait DecimalToInteger<T> {
    fn to_uint(self, precision: impl Into<u32>) -> StdResult<T>;
}

impl DecimalToInteger<Uint128> for Decimal256 {
    fn to_uint(self, precision: impl Into<u32>) -> StdResult<Uint128> {
        let multiplier = Uint256::from(10u8).pow(precision.into());
        (multiplier * self.numerator() / self.denominator())
            .try_into()
            .map_err(|err| StdError::generic_err(format!("{err}")))
    }
}

pub trait ConvertInto<T>
where
    Self: Sized,
{
    type Error: Into<StdError>;
    fn conv(self) -> Result<T, Self::Error>;
}

impl ConvertInto<Decimal> for Decimal256 {
    type Error = StdError;

    fn conv(self) -> Result<Decimal, Self::Error> {
        let numerator: Uint128 = self.numerator().try_into()?;
        Decimal::from_atomics(numerator, self.decimal_places())
            .map_err(|err| StdError::generic_err(err.to_string()))
    }
}
