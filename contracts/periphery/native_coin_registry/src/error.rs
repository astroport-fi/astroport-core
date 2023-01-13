use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Duplicate coins are provided")]
    DuplicateCoins {},

    #[error("The coin cannot have zero precision: {0}")]
    CoinWithZeroPrecision(String),

    #[error("The coin does not exist: {0}")]
    CoinDoesNotExist(String),
}
