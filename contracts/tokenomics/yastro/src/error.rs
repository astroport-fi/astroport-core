use cosmwasm_std::{OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("{0}")]
    Cw20(#[from] cw20_base::ContractError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount must be greater than zero")]
    ZeroAmount {},

    #[error("{denom} isn't accepted as a reward")]
    NotARewardDenom { denom: String },

    #[error("{denom} is staked, so it can't also be a reward")]
    StakedDenomAsReward { denom: String },

    #[error("Too many open unbondings: withdraw the matured ones first")]
    TooManyUnbondings {},

    #[error("Nothing to withdraw yet")]
    NothingToWithdraw {},

    #[error("No rewards to claim")]
    NothingToClaim {},

    #[error("{pool} isn't a registered pool")]
    PoolNotRegistered { pool: String },

    #[error("Incentives isn't set")]
    IncentivesNotSet {},

    #[error("The unbonding period must be greater than zero")]
    ZeroUnbondingPeriod {},

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
