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

    #[error("Invalid number of assets infos. Must be exactly 2")]
    InvalidNumberOfAssets {},

    #[error("Operation is not supported")]
    NotSupported {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

    #[error("CW20 tokens are not supported")]
    NonNativeAsset {},

    #[error("Denom {0} doesn't belong to the supervault")]
    InvalidDenom(String),

    #[error("belief_price is mandatory field")]
    MissingPrice {},

    #[error("Incentives contract is not set in the factory")]
    IncentivesNotFound {},

    #[error("Slippage is more than expected: received {0}, expected {1} LP tokens")]
    ProvideSlippageViolation(Uint128, Uint128),

    #[error("Received {received} {asset_name} but expected {expected}")]
    WithdrawSlippageViolation {
        asset_name: String,
        received: Uint128,
        expected: Uint128,
    },

    #[error("Wrong asset length: expected {expected}, actual {actual}")]
    WrongAssetLength { expected: usize, actual: usize },

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},
}
