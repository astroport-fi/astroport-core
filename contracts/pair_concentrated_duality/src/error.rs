use cosmwasm_std::{ConversionOverflowError, OverflowError, StdError, Uint128};
use cw_utils::{ParseReplyError, PaymentError};
use thiserror::Error;

use astroport::asset::MINIMUM_LIQUIDITY_AMOUNT;
use astroport::pair::MAX_FEE_SHARE_BPS;
use astroport_pcl_common::error::PclError;

use crate::orderbook::error::OrderbookError;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PclError(#[from] PclError),

    #[error("{0}")]
    OrderbookError(#[from] OrderbookError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Pair is not registered in the factory. Only swap and withdraw are allowed")]
    PairIsNotRegistered {},

    #[error("Invalid number of assets. This pair supports only {0} assets")]
    InvalidNumberOfAssets(usize),

    #[error("Initial provide can not be one-sided")]
    InvalidZeroAmount {},

    #[error("Initial liquidity must be more than {}", MINIMUM_LIQUIDITY_AMOUNT)]
    MinimumLiquidityAmountError {},

    #[error(
        "Fee share is 0 or exceeds maximum allowed value of {} bps",
        MAX_FEE_SHARE_BPS
    )]
    FeeShareOutOfBounds {},

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},

    #[error("cw20 tokens are not supported")]
    NonNativeAsset {},

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
}
