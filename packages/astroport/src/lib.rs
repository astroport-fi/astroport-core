use cosmwasm_std::{Decimal, Decimal256, StdError, StdResult, Uint128};

pub use decimal_checked_ops::DecimalCheckedOps;
pub use uints::U256;

pub mod asset;
pub mod common;
pub mod cosmwasm_ext;
pub mod factory;
pub mod fee_granter;
#[cfg(feature = "injective")]
pub mod injective_ext;
pub mod maker;
pub mod native_coin_registry;
pub mod observation;
pub mod oracle;
pub mod pair;
pub mod pair_concentrated;
pub mod pair_concentrated_inj;
pub mod pair_xyk_sale_tax;
pub mod querier;
pub mod restricted_vector;
pub mod router;
pub mod staking;
pub mod token;
pub mod token_factory;
pub mod tokenfactory_tracker;
pub mod vesting;
pub mod xastro_token;

#[cfg(test)]
mod mock_querier;

pub mod astro_converter;
pub mod incentives;
pub mod pair_xastro;
#[cfg(test)]
mod testing;

#[allow(clippy::all)]
mod uints {
    use uint::construct_uint;

    construct_uint! {
        pub struct U256(4);
    }
}

mod decimal_checked_ops {
    use std::convert::TryInto;

    use cosmwasm_std::{Decimal, Fraction, OverflowError, Uint128, Uint256};

    pub trait DecimalCheckedOps {
        fn checked_add(self, other: Decimal) -> Result<Decimal, OverflowError>;
        fn checked_mul_uint128(self, other: Uint128) -> Result<Uint128, OverflowError>;
    }

    impl DecimalCheckedOps for Decimal {
        fn checked_add(self, other: Decimal) -> Result<Decimal, OverflowError> {
            self.numerator()
                .checked_add(other.numerator())
                .map(|_| self + other)
        }
        fn checked_mul_uint128(self, other: Uint128) -> Result<Uint128, OverflowError> {
            if self.is_zero() || other.is_zero() {
                return Ok(Uint128::zero());
            }
            let multiply_ratio =
                other.full_mul(self.numerator()) / Uint256::from(self.denominator());
            if multiply_ratio > Uint256::from(Uint128::MAX) {
                Err(OverflowError::new(
                    cosmwasm_std::OverflowOperation::Mul,
                    self,
                    other,
                ))
            } else {
                Ok(multiply_ratio.try_into().unwrap())
            }
        }
    }
}

/// Converts [`Decimal256`] to [`Decimal`].
pub fn to_decimal(value: Decimal256) -> StdResult<Decimal> {
    let atomics = Uint128::try_from(value.atomics())?;
    Decimal::from_atomics(atomics, value.decimal_places()).map_err(|_| {
        StdError::generic_err(format!("Failed to convert Decimal256 {} to Decimal", value))
    })
}
