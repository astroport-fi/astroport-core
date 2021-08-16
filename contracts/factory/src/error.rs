use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pair code id is not allowed")]
    PairCodeNotAllowed {},

    #[error("Pair was already registered")]
    PairWasRegistered {},
}
