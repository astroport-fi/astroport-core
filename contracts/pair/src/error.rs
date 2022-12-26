use astroport::asset::MINIMUM_LIQUIDITY_AMOUNT;
use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("CW20 tokens can be swapped via Cw20::Send message only")]
    Cw20DirectSwap {},

    #[error("Operation non supported")]
    NonSupported {},

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Provided spread amount exceeds allowed limit")]
    AllowedSpreadAssertion {},

    #[error("Operation exceeds max splippage tolerance")]
    MaxSlippageAssertion {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("Asset mismatch between the requested and the stored asset in contract")]
    AssetMismatch {},

    #[error("Pair type mismatch. Check factory pair configs")]
    PairTypeMismatch {},

    #[error("Generator address is not set in factory. Cannot auto-stake")]
    AutoStakeError {},

    #[error("Initial liquidity must be more than {}", MINIMUM_LIQUIDITY_AMOUNT)]
    MinimumLiquidityAmountError {},

    #[error("Failed to migrate the contract")]
    MigrationError {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
