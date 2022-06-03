use cosmwasm_std::{Decimal, Decimal256, StdError, StdResult};

/// ## Description
/// Converts [`Decimal`] to [`Decimal256`].
pub fn decimal2decimal256(dec_value: Decimal) -> StdResult<Decimal256> {
    Decimal256::from_atomics(dec_value.atomics(), dec_value.decimal_places()).map_err(|_| {
        StdError::generic_err(format!(
            "Failed to convert Decimal {} to Decimal256",
            dec_value
        ))
    })
}
