use cosmwasm_std::{
    CheckedFromRatioError, ConversionOverflowError, OverflowError, StdError, Uint128,
};
use cw_utils::PaymentError;
use thiserror::Error;

use astroport::factory::PairType;
use astroport::incentives::MAX_REWARD_TOKENS;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    CheckedFromRatioError(#[from] CheckedFromRatioError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Duplicated pool found")]
    DuplicatedPoolFound {},

    #[error("Amount to withdraw {withdraw_amount} exceeds balance {available}")]
    AmountExceedsBalance {
        available: Uint128,
        withdraw_amount: Uint128,
    },

    #[error("User {user} doesn't have position in {lp_token}")]
    PositionDoesntExist { user: String, lp_token: String },

    #[error("Pool {pool} doesn't have {reward} reward")]
    RewardNotFound { pool: String, reward: String },

    #[error("Too many reward tokens in pool {lp_token}. Maximum allowed is {MAX_REWARD_TOKENS}")]
    TooManyRewardTokens { lp_token: String },

    #[error("Incentivization fee {fee} expected as you are trying to add new reward token {new_reward_token} for pool {lp_token}")]
    IncentivizationFeeExpected {
        fee: String,
        lp_token: String,
        new_reward_token: String,
    },

    #[error("Token {token} is blocked")]
    BlockedToken { token: String },

    #[error("Pair type {pair_type} is blocked")]
    BlockedPairType { pair_type: PairType },

    #[error("Failed to parse or process reply message")]
    FailedToParseReply {},

    #[error("No orphaned rewards to claim")]
    NoOrphanedRewards {},

    #[error("Failed to set 0 alloc point for pool {lp_token}")]
    ZeroAllocPoint { lp_token: String },

    #[error("Failed to migrate contract")]
    MigrationError {},

    #[error("Sent insufficient reward {reward} for pool {lp_token}")]
    InsuffiicientRewardToken { reward: String, lp_token: String },
}
