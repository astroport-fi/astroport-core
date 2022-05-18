use cosmwasm_std::{DivideByZeroError, OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes staking contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
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
