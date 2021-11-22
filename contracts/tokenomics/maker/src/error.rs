use astroport::asset::AssetInfo;
use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("TokenMaker: Invalid bridge")]
    InvalidBridge {},

    #[error("Insufficient balance in contract to process claim")]
    BalanceTooSmall {},

    #[error("Repetitive reply definition")]
    RepetitiveReply {},

    #[error("Cannot swap {0} to {1}. Pair not found in factory")]
    PairNotFound(AssetInfo, AssetInfo),

    #[error("Incorrect governance percent of its share")]
    IncorrectGovernancePercent {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
