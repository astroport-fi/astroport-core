use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// This enum describes generator contract errors!
#[derive(Error, Debug, PartialEq)]
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
    ZeroOrphanRewards {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("The pool already has a reward proxy contract!")]
    PoolAlreadyHasRewardProxyContract {},

    #[error("Generator is disabled!")]
    GeneratorIsDisabled {},

    #[error("Duplicate of pool")]
    PoolDuplicate {},

    #[error("Pair is not registered in factory!")]
    PairNotRegistered {},

    #[error("ASTRO or native assets cannot be blocked! You are trying to block {asset}")]
    AssetCannotBeBlocked { asset: String },

    #[error("Maximum generator limit exceeded!")]
    GeneratorsLimitExceeded {},

    #[error("You can not withdraw 0 LP tokens.")]
    ZeroWithdraw {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
