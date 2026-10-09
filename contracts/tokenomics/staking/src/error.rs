use cosmwasm_std::{StdError, Uint128};
use cw_utils::{ParseReplyError, PaymentError};
use thiserror::Error;

use crate::contract::MINIMUM_STAKE_AMOUNT;

/// This enum describes staking contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("Initial stake amount must be more than {MINIMUM_STAKE_AMOUNT}")]
    MinimumStakeAmountError {},

    #[error("Insufficient amount of Stake")]
    StakeAmountTooSmall {},

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Staking is paused")]
    Paused {},

    #[error("Staking is leave-only: new ASTRO can't be staked")]
    LeaveOnly {},

    #[error("ASTRO can only be withdrawn while staking is paused")]
    WithdrawWhileNotPaused {},

    #[error("Can't withdraw {amount} ASTRO: the contract holds {balance}")]
    WithdrawExceedsBalance { amount: Uint128, balance: Uint128 },

    #[error("Can't withdraw zero ASTRO")]
    WithdrawZero {},

    #[error("ASTRO was withdrawn from staking, so it can only stay paused")]
    Retired {},
}
