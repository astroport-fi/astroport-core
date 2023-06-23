use cosmwasm_std::{StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Amount in message {expected} doesn't match sent amount {actual}")]
    InvalidAmount { expected: Uint128, actual: Uint128 },

    #[error("Unauthorized")]
    Unauthorized {},
}
