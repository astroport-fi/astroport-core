use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("Only {denom} can be sent to the burner")]
    UnexpectedFunds { denom: String },

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
