use cosmwasm_std::{Decimal, Decimal256, Uint128, Uint256, Uint64};
use std::ops;
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

pub trait OneValue
where
    Self: From<u8>,
{
    fn one() -> Self {
        Self::from(1u8)
    }
}

impl AbsDiff for Uint256 {}
impl AbsDiff for Uint128 {}
impl AbsDiff for Uint64 {}
impl AbsDiff for Decimal {}
impl AbsDiff for Decimal256 {}

impl OneValue for Uint256 {}
impl OneValue for Uint128 {}
impl OneValue for Uint64 {}
