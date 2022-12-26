use crate::math::{MAX_AMP, MAX_AMP_CHANGE, MIN_AMP_CHANGING_TIME};
use astroport::asset::MINIMUM_LIQUIDITY_AMOUNT;
use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes stableswap pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("CW20 tokens can be swapped via Cw20::Send message only")]
    Cw20DirectSwap {},

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

    #[error("Operation exceeds max splippage tolerance")]
    MaxSlippageAssertion {},

    #[error("Native token balance mismatch between the argument and the transferred")]
    AssetMismatch {},

    #[error("Pair type mismatch. Check factory pair configs")]
    PairTypeMismatch {},

    #[error(
        "Amp coefficient must be greater than 0 and less than or equal to {}",
        MAX_AMP
    )]
    IncorrectAmp {},

    #[error(
        "The difference between the old and new amp value must not exceed {} times",
        MAX_AMP_CHANGE
    )]
    MaxAmpChangeAssertion {},

    #[error(
        "Amp coefficient cannot be changed more often than once per {} seconds",
        MIN_AMP_CHANGING_TIME
    )]
    MinAmpChangingTimeAssertion {},

    #[error("You need to provide init params")]
    InitParamsNotFound {},

    #[error("Generator address is not set in factory. Cannot autostake")]
    AutoStakeError {},

    #[error("It is not possible to provide liquidity with one token for an empty pool")]
    InvalidProvideLPsWithSingleToken {},

    #[error("Initial liquidity must be more than {}", MINIMUM_LIQUIDITY_AMOUNT)]
    MinimumLiquidityAmountError {},

    #[error("Contract can't be migrated!")]
    MigrationError {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
