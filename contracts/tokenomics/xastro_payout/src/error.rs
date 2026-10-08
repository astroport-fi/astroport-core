use cosmwasm_std::{OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid Merkle root: expected a hex sha256 hash")]
    InvalidRoot {},

    #[error("Invalid proof for {address}")]
    InvalidProof { address: String },

    #[error("Paying out more than the list total")]
    TotalExceeded {},

    #[error("The payout is closed")]
    Closed {},

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
