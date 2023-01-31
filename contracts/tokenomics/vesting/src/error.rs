use cosmwasm_std::{Addr, OverflowError, StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

/// ## Description
/// This enum describes generator vesting contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount is not available!")]
    AmountIsNotAvailable {},

    #[error("Vesting schedule error on addr: {0}. Should satisfy: (start < end and at_start < total) or (start = end and at_start = total)")]
    VestingScheduleError(Addr),

    #[error("Vesting schedule amount error. The total amount should be equal to the CW20 receive amount.")]
    VestingScheduleAmountError {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Failed to withdraw tokens due to multiple active schedules for account {0}")]
    MultipleActiveSchedules(String),

    #[error("Account {0} has no active vesting schedule")]
    NoActiveVestingSchedule(String),

    #[error("Failed to withdraw from active schedule: amount left {0}")]
    NotEnoughTokens(Uint128),
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
