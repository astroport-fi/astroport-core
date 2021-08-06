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
}
