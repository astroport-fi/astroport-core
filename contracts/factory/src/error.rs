use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pair was already created")]
    PairWasCreated {},

    #[error("Pair was already registered")]
    PairWasRegistered {},

    #[error("Fee bps in pair config must be smaller than or equal to 10,000")]
    PairConfigInvalidFeeBps {},

    #[error("Pair config not found")]
    PairConfigNotFound {},
}
