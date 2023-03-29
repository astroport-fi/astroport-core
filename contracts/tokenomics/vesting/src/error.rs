use cosmwasm_std::{OverflowError, StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes generator vesting contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Withdrawn amount must not be zero")]
    ZeroAmountWithdrawal {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount is not available!")]
    AmountIsNotAvailable {},

    #[error("Vesting schedule error on addr: {0}. Should satisfy: (start < end, end > current_time and start_amount < end_amount)")]
    VestingScheduleError(String),

    #[error(
        "Vesting schedule amount error. The total amount should be equal to the received amount."
    )]
    VestingScheduleAmountError {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Failed to withdraw tokens due to multiple active schedules for account {0}")]
    MultipleActiveSchedules(String),

    #[error("Account {0} has no active vesting schedule")]
    NoActiveVestingSchedule(String),

    #[error("For account {0} number of schedules exceeds maximum limit")]
    ExceedSchedulesMaximumLimit(String),

    #[error("Failed to withdraw from active schedule: amount left {0}")]
    NotEnoughTokens(Uint128),
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
