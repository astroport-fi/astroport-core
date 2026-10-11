use cosmwasm_std::{Decimal, OverflowError, StdError, Uint128};
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

    #[error("{address} isn't a pool holding yASTRO")]
    NotAPool { address: String },

    #[error("{address} hasn't enabled claim_for")]
    ClaimForNotEnabled { address: String },

    #[error("{staked} of {supply} {lp_token} staked in Incentives, under the {min_share} share needed to forward")]
    NotEnoughLpStaked {
        lp_token: String,
        staked: Uint128,
        supply: Uint128,
        min_share: Decimal,
    },

    #[error("The minimum staked LP share can't be more than 1")]
    InvalidMinLpStaked {},

    #[error("Every reward is still too small to schedule in Incentives")]
    NothingToForward {},

    #[error("yASTRO can't be sent to its own contract")]
    SelfRecipient {},

    #[error("At most {max} reward denoms, ever")]
    TooManyRewardDenoms { max: usize },

    #[error("Incentives isn't set")]
    IncentivesNotSet {},

    #[error("The unbonding period must be between 1 second and {max} seconds")]
    InvalidUnbondingPeriod { max: u64 },

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
