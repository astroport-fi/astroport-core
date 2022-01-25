use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes generator contract errors!
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Insufficient balance in contract to process claim")]
    BalanceTooSmall {},

    #[error("Pool with the LP token already exists!")]
    TokenPoolAlreadyExists {},

    #[error("Reward proxy not allowed!")]
    RewardProxyNotAllowed {},

    #[error("Pool doesn't have additional rewards!")]
    PoolDoesNotHaveAdditionalRewards {},

    #[error("Insufficient amount of orphan rewards!")]
    OrphanRewardsTooSmall {},

    #[error("Contract can't be migrated!")]
    MigrationError {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
