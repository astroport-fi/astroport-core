use crate::cosmwasm_ext::ConvertInto;
use cosmwasm_std::{Decimal, Decimal256, StdError, Uint128, Uint256};
use injective_math::FPDecimal;
use std::str::FromStr;
use thiserror::Error;

pub const FPDECIMAL_DENOMINATOR: u128 = 1000000000000000000;

#[derive(Error, Debug, PartialEq)]
pub enum InjMathError {
    #[error("Negative value")]
    NegativeValue,
    #[error("Overflow")]
    Overflow,
}

impl From<InjMathError> for String {
    fn from(err: InjMathError) -> String {
        match err {
            InjMathError::NegativeValue => "Negative value".to_string(),
            InjMathError::Overflow => "Overflow".to_string(),
        }
    }
}

impl From<InjMathError> for StdError {
    fn from(value: InjMathError) -> Self {
        StdError::generic_err(format!(
            "FPDecimal conversion error: {}",
            String::from(value)
        ))
    }
}

impl ConvertInto<Decimal> for FPDecimal {
    type Error = InjMathError;

    fn conv(self) -> Result<Decimal, Self::Error> {
        if self.sign == 0 {
            return Err(InjMathError::NegativeValue);
        }

        let mut bytes: [u8; 32] = [0; 32];
        self.num.to_little_endian(&mut bytes);
        let num =
            Uint128::try_from(Uint256::from_le_bytes(bytes)).map_err(|_| InjMathError::Overflow)?;
        Ok(Decimal::from_ratio(num, FPDECIMAL_DENOMINATOR))
    }
}

impl ConvertInto<Decimal256> for FPDecimal {
    type Error = InjMathError;

    fn conv(self) -> Result<Decimal256, Self::Error> {
        if self.sign == 0 {
            return Err(InjMathError::NegativeValue);
        }

        let mut bytes: [u8; 32] = [0; 32];
        self.num.to_little_endian(&mut bytes);
        Ok(Decimal256::from_ratio(
            Uint256::from_le_bytes(bytes),
            FPDECIMAL_DENOMINATOR,
        ))
    }
}

impl ConvertInto<FPDecimal> for Decimal {
    type Error = StdError;

    fn conv(self) -> Result<FPDecimal, Self::Error> {
        FPDecimal::from_str(&self.to_string())
    }
}

impl ConvertInto<FPDecimal> for Decimal256 {
    type Error = StdError;

    fn conv(self) -> Result<FPDecimal, Self::Error> {
        FPDecimal::from_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Decimal256;
    use std::str::FromStr;

    #[test]
    fn test_fpdecimal_to_decimal() {
        let fp_dec = FPDecimal::from(1u128);
        let dec: Decimal = fp_dec.conv().unwrap();
        let dec256: Decimal256 = fp_dec.conv().unwrap();
        assert_eq!(dec, Decimal::one());
        assert_eq!(dec256, Decimal256::one());

        let fp_dec = FPDecimal::from(-1i128);
        assert_eq!(
            <FPDecimal as ConvertInto<Decimal>>::conv(fp_dec).unwrap_err(),
            InjMathError::NegativeValue
        );
        assert_eq!(
            <FPDecimal as ConvertInto<Decimal256>>::conv(fp_dec).unwrap_err(),
            InjMathError::NegativeValue
        );

        assert_eq!(
            <FPDecimal as ConvertInto<Decimal>>::conv(FPDecimal::MAX).unwrap_err(),
            InjMathError::Overflow
        );
        let dec256: Decimal256 = FPDecimal::MAX.conv().unwrap();
        assert_eq!(
            dec256,
            Decimal256::from_str(
                "115792089237316195423570985008687907853269984665640564039457.584007913129639935"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_decimal_to_fpdecimal() {
        let dec: FPDecimal = Decimal::one().conv().unwrap();
        assert_eq!(dec, FPDecimal::ONE);
        let dec256: FPDecimal = Decimal256::one().conv().unwrap();
        assert_eq!(dec256, FPDecimal::ONE);
    }
}
