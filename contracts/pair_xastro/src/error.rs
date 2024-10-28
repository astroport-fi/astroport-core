use crate::contract::MINIMUM_STAKE_AMOUNT;
use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Operation is not supported")]
    NotSupported {},

    #[error("Invalid asset {0}")]
    InvalidAsset(String),

    #[error("Invalid unstake amount. Want {want} but staking contract has only {total}")]
    InvalidUnstakeAmount { want: Uint128, total: Uint128 },

    #[error("Initial stake amount must be more than {MINIMUM_STAKE_AMOUNT}")]
    MinimumStakeAmountError {},
}
