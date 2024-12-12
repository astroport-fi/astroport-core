use cosmwasm_std::{ConversionOverflowError, OverflowError, StdError, Uint128};
use cw_utils::{ParseReplyError, PaymentError};
use thiserror::Error;

use astroport::{asset::MINIMUM_LIQUIDITY_AMOUNT, pair::MAX_FEE_SHARE_BPS};
use astroport_pcl_common::error::PclError;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    PclError(#[from] PclError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("CW20 tokens can be swapped via Cw20::Send message only")]
    Cw20DirectSwap {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

    #[error("Initial provide can not be one-sided")]
    InvalidZeroAmount {},

    #[error("Initial liquidity must be more than {}", MINIMUM_LIQUIDITY_AMOUNT)]
    MinimumLiquidityAmountError {},

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},

    #[error("Pair is not registered in the factory. Only swap and withdraw are allowed")]
    PairIsNotRegistered {},

    #[error("Invalid number of assets. This pair supports only {0} assets")]
    InvalidNumberOfAssets(usize),

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error(
        "Fee share is 0 or exceeds maximum allowed value of {} bps",
        MAX_FEE_SHARE_BPS
    )]
    FeeShareOutOfBounds {},

    #[error("Slippage is more than expected: received {0}, expected {1} LP tokens")]
    ProvideSlippageViolation(Uint128, Uint128),
}
