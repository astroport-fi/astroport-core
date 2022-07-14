use cosmwasm_std::{Addr, OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes generator vesting contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount is not available!")]
    AmountIsNotAvailable {},

    #[error("Vesting schedule error on addr: {0}. Should satisfy: (start < end and at_start < total) or (start = end and at_start = total)")]
    VestingScheduleError(Addr),

    #[error("Vesting schedule amount error. The total amount should be equal to the CW20 receive amount.")]
    VestingScheduleAmountError,
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
