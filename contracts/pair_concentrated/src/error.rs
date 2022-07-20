use cosmwasm_std::{
    CheckedFromRatioError, CheckedMultiplyRatioError, ConversionOverflowError, OverflowError,
    StdError,
};
use thiserror::Error;

use crate::constants::MIN_AMP_CHANGING_TIME;

/// ## Description
/// This enum describes stableswap pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    CheckedMultiplyRatioError(#[from] CheckedMultiplyRatioError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Insufficient amount of liquidity")]
    LiquidityAmountTooSmall {},

    #[error("Provided spread amount exceeds allowed limit")]
    AllowedSpreadAssertion {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Native token balance mismatch between the argument and the transferred")]
    AssetMismatch {},

    #[error("{0} parameter must be greater than {1} and less than or equal to {2}")]
    IncorrectPoolParam(String, u128, u128),

    #[error(
        "{0} error: The difference between the old and new amp or gamma values must not exceed {1} percent",
    )]
    MaxChangeAssertion(String, u128),

    #[error(
        "Amp and gamma coefficients cannot be changed more often than once per {} seconds",
        MIN_AMP_CHANGING_TIME
    )]
    MinChangingTimeAssertion {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

    #[error("Generator address is not set in factory. Cannot autostake")]
    AutoStakeError {},

    #[error("It is not possible to provide liquidity with one token for an empty pool")]
    InvalidProvideLPsWithSingleToken {},

    #[error("Pair is not migrated to the new admin!")]
    PairIsNotMigrated {},

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Ask or offer asset is missed")]
    VariableAssetMissed {},

    #[error("Source and target assets are the same")]
    SameAssets {},

    #[error("Invalid number of assets. This pair support only {0} assets")]
    InvalidNumberOfAssets(usize),

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}

impl From<ConversionOverflowError> for ContractError {
    fn from(o: ConversionOverflowError) -> Self {
        StdError::from(o).into()
    }
}

impl From<CheckedFromRatioError> for ContractError {
    fn from(o: CheckedFromRatioError) -> Self {
        StdError::generic_err(o.to_string()).into()
    }
}
