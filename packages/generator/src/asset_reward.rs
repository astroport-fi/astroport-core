use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};

// These enumerations are temporary placed here until any pair with its asset rewards is implemented,
// similar to bLUNA <-> LUNA pair was

#[cw_serde]
pub enum ExecuteMsg {
    ClaimReward {
        /// An address which will receive the bLUNA reward
        receiver: Option<String>,
    },
    /// Claims the bLUNA reward for a user that deposited their LP tokens in the Generator contract
    ClaimRewardByGenerator {
        /// The user whose LP tokens are/were staked in the Generator
        user: String,
        /// The user's LP token amount before the LP token transfer between their wallet and the Generator
        user_share: Uint128,
        /// The total LP token amount already deposited by all users in the Generator
        total_share: Uint128,
    },
    /// Callback for distributing bLUNA rewards
    HandleReward {
        previous_reward_balance: Uint128,
        user: Addr,
        user_share: Uint128,
        total_share: Uint128,
        receiver: Option<Addr>,
    },
}

#[cw_serde]
pub enum QueryMsg {
    /// Returns pending token rewards that can be claimed by a specific user using a [`Asset`] object.
    PendingReward { user: String },
}
