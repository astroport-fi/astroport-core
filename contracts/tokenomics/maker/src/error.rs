use astroport::asset::AssetInfo;
use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes maker contract errors!
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid bridge. {0} to {1} not found")]
    InvalidBridge(AssetInfo, AssetInfo),

    #[error("Insufficient balance in contract to process claim")]
    BalanceTooSmall {},

    #[error("Empty reply definition")]
    EmptyReply {},

    #[error("Cannot swap {0}. No swap destinations")]
    CannotSwap(AssetInfo),

    #[error("Incorrect governance percent of its share")]
    IncorrectGovernancePercent {},

    #[error("Incorrect max spread")]
    IncorrectMaxSpread {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
