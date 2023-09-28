use cosmwasm_std::{DivideByZeroError, OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Proposal is not open")]
    NotOpen {},

    #[error("Proposal voting period has expired")]
    Expired {},

    #[error("Proposal must expire before you can close it")]
    NotExpired {},

    #[error("Wrong expiration option")]
    WrongExpiration {},

    #[error("Already voted on this proposal")]
    AlreadyVoted {},

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},

    #[error("Cannot close completed or passed proposals")]
    WrongCloseStatus {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Target pool is not set")]
    TargetPoolError {},

    #[error("Target pool is already set")]
    TargetPoolIsAlreadySet {},

    #[error("Target pool is not empty")]
    TargetPoolAmountError {},

    #[error("Withdraw all LP tokens from the generator before migrating the target pool")]
    GeneratorAmountError {},

    #[error("Migration pool is not set")]
    MigrationPoolError {},

    #[error("Migration pool is already set")]
    MigrationPoolIsAlreadySet {},

    #[error("Complete migration from the target pool")]
    MigrationNotCompleted {},

    #[error("Target and migration pools cannot be the same")]
    PoolsError {},

    #[error("Unsupported pair type. Allowed pair types are: xyk, concentrated")]
    PairTypeError {},

    #[error("Operation is unavailable. Rage quit has already started")]
    RageQuitStarted {},

    #[error("Operation is unavailable. Rage quit is not started")]
    RageQuitIsNotStarted {},

    #[error("Unauthorized: {0} cannot transfer {1}")]
    UnauthorizedTransfer(String, String),

    #[error("The asset {0} does not belong to the target pool")]
    InvalidAsset(String),

    #[error("CW20 tokens unsupported in the target pool. Use native token instead")]
    UnsupportedCw20 {},

    #[error(
        "Asset balance mismatch between the argument and the Multisig balance. \
    Available Multisig balance for {0}: {1}"
    )]
    AssetBalanceMismatch(String, String),

    #[error("Insufficient balance for: {0}. Available balance: {1}")]
    BalanceToSmall(String, String),

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Claim all rewards from the generator before migrating the target pool")]
    ClaimAmountError {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}

impl From<DivideByZeroError> for ContractError {
    fn from(err: DivideByZeroError) -> Self {
        StdError::from(err).into()
    }
}
