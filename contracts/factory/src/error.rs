use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pair was already registered")]
    PairWasRegistered {},

    #[error("Duplicate of pair configs")]
    PairConfigDuplicate {},

    #[error("Pair config not found")]
    PairConfigNotFound {},
}
