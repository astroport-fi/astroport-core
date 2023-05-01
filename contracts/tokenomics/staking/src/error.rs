use crate::contract::MINIMUM_STAKE_AMOUNT;
use cosmwasm_std::{DivideByZeroError, OverflowError, StdError};
use thiserror::Error;

/// This enum describes staking contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("An error occurred during migration")]
    MigrationError {},

    #[error("Initial stake amount must be more than {}", MINIMUM_STAKE_AMOUNT)]
    MinimumStakeAmountError {},

    #[error("Insufficient amount of Stake")]
    StakeAmountTooSmall {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}

impl From<DivideByZeroError> for ContractError {
    fn from(err: DivideByZeroError) -> Self {
        StdError::from(err).into()
    }
}
