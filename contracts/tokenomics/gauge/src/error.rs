use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

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
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
