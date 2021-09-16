use astroport::asset::AssetInfo;
use cosmwasm_std::StdError;
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

    #[error("Incorrent governance percent of its share")]
    IncorrectGovernancePercent {},
}
