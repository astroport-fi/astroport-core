use cosmwasm_std::{Decimal, StdError};
use thiserror::Error;

use crate::consts::MIN_AMP_CHANGING_TIME;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum PclError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0} parameter must be greater than {1} and less than or equal to {2}")]
    IncorrectPoolParam(String, String, String),

    #[error(
    "{0} error: The difference between the old and new amp or gamma values must not exceed {1} percent",
    )]
    MaxChangeAssertion(String, Decimal),

    #[error(
        "Amp and gamma coefficients cannot be changed more often than once per {} seconds",
        MIN_AMP_CHANGING_TIME
    )]
    MinChangingTimeAssertion {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Generator address is not set in factory. Cannot auto-stake")]
    AutoStakeError {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Provided spread amount exceeds allowed limit")]
    AllowedSpreadAssertion {},

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),
}
