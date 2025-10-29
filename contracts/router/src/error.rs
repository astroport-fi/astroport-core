use cosmwasm_std::{OverflowError, StdError, Uint128};
use thiserror::Error;

/// This enum describes oracle contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error(
        "The next offer asset must be the same as the previous ask asset; \
    {prev_ask_asset} --> {next_offer_asset} --> {next_ask_asset}"
    )]
    InvalidPathOperations {
        prev_ask_asset: String,
        next_offer_asset: String,
        next_ask_asset: String,
    },

    #[error("Doubling assets in one batch of path; {offer_asset} --> {ask_asset}")]
    DoublingAssetsPath {
        offer_asset: String,
        ask_asset: String,
    },

    #[error("Must specify swap operations!")]
    MustProvideOperations {},

    #[error("Assertion failed; minimum receive amount: {receive}, swap amount: {amount}")]
    AssertionMinimumReceive { receive: Uint128, amount: Uint128 },
}
