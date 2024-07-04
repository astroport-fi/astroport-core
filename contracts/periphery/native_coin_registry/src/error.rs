use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Duplicate coins are provided")]
    DuplicateCoins {},

    #[error("Invalid decimals {decimals} for {denom}. Allowed: 0-18")]
    InvalidDecimals { denom: String, decimals: u8 },

    #[error("The coin does not exist: {0}")]
    CoinDoesNotExist(String),

    #[error("The coin already exists: {0}")]
    CoinAlreadyExists(String),

    #[error("You must send 1 {0} unit")]
    MustSendCoin(String),
}
