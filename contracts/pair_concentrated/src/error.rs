use crate::consts::MIN_AMP_CHANGING_TIME;
use astroport::asset::MINIMUM_LIQUIDITY_AMOUNT;
use cosmwasm_std::{ConversionOverflowError, Decimal, OverflowError, StdError};
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("CW20 tokens can be swapped via Cw20::Send message only")]
    Cw20DirectSwap {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

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

    #[error("Initial provide can not be one-sided")]
    InvalidZeroAmount {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Provided spread amount exceeds allowed limit")]
    AllowedSpreadAssertion {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("Generator address is not set in factory. Cannot auto-stake")]
    AutoStakeError {},

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

    #[error("Asset balances tracking is already enabled")]
    AssetBalancesTrackingIsAlreadyEnabled {},
}
