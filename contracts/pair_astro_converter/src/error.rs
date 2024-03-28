use cosmwasm_std::StdError;
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Operation is not supported")]
    NotSupported {},

    #[error("CW20 tokens can be swapped via Cw20::Send message only")]
    Cw20DirectSwap {},

    #[error("Failed to migrate from {current}. Expected: {expected}")]
    MigrationError { expected: String, current: String },

    #[error("This pair swaps from old ASTRO ({old}) to new ASTRO only ({new})")]
    AssetMismatch { old: String, new: String },
}
