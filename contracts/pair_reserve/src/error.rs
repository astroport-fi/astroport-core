use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Operation is not supported")]
    NonSupported {},

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Provided spread amount exceeds allowed limit")]
    AllowedSpreadAssertion {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("{0} validation error: {1}")]
    ValidationError(String, String),

    #[error("Failed to swap because return amount is zero")]
    SwapZeroAmount {},

    #[error("Ask pool is empty")]
    AskPoolEmpty {},

    #[error("Failed to retrieve the asset price from the oracles")]
    OraclesError {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
