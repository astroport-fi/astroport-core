use cosmwasm_std::{CheckedMultiplyRatioError, DivideByZeroError, OverflowError, StdError};
use thiserror::Error;

/// This enum describes maker contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    DivideByZero(#[from] DivideByZeroError),

    #[error("{0}")]
    CheckedMultiplyRatio(#[from] CheckedMultiplyRatioError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Incorrect max spread")]
    IncorrectMaxSpread {},

    #[error("Incorrect cooldown. Min: {min}, Max: {max}")]
    IncorrectCooldown { min: u64, max: u64 },

    #[error("Collect cooldown is not expired. Next collect is possible at {next_collect_ts}")]
    Cooldown { next_collect_ts: u64 },

    #[error("There must be between 1 and {max} legs")]
    InvalidLegCount { max: usize },

    #[error("Leg shares must be greater than zero and add up to exactly 1, got {total}")]
    InvalidLegShares { total: String },

    #[error("Leg {reason}")]
    InvalidLeg { reason: String },

    #[error("Route {reason}")]
    InvalidRoute { reason: String },

    #[error("No route from {asset} to the base asset")]
    NoRoute { asset: String },

    #[error("Duplicate asset {asset} in the list")]
    DuplicateAsset { asset: String },

    #[error("Duplicate collector {address}")]
    DuplicateCollector { address: String },

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
