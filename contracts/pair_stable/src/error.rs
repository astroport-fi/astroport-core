use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Operation exceeds max spread limit")]
    MaxSpreadAssertion {},

    #[error("Operation exceeds max splippage tolerance")]
    MaxSlippageAssertion {},

    #[error("Native token balance mismatch between the argument and the transferred")]
    AssetMismatch {},

    #[error("Pair type mismatch. Check factory pair configs")]
    PairTypeMismatch {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
