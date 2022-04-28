use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::Asset;

use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity allows an account to provide liquidity in a pool with bLUNA
    ProvideLiquidity {
        /// The two assets available in the pool
        assets: [Asset; 2],
        /// The slippage tolerance that allows liquidity provision only if the price in the pool doesn't move too much
        slippage_tolerance: Option<Decimal>,
        /// Determines whether the LP tokens minted for the user is auto_staked in the Generator contract
        auto_stake: Option<bool>,
        /// The receiver of LP tokens
        receiver: Option<String>,
    },
    /// Swap performs a swap in the pool
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Update the pair configuration
    UpdateConfig { params: Binary },
    /// Claims bLUNA rewards and sends them to the specified receiver
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

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`super::asset::PairInfo`].
    Pair {},
    /// Returns information about a pool in an object of type [`super::pair::PoolResponse`].
    Pool {},
    /// Returns contract configuration settings in a custom [`super::pair::ConfigResponse`] structure.
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    Share { amount: Uint128 },
    /// Returns information about a swap simulation in a [`super::pair::SimulationResponse`] object.
    Simulation { offer_asset: Asset },
    /// Returns information about a reverse simulation in a [`super::pair::ReverseSimulationResponse`] object.
    ReverseSimulation { ask_asset: Asset },
    /// Returns information about cumulative prices (used for TWAPs) in a [`super::pair::CumulativePricesResponse`] object.
    CumulativePrices {},
    /// Returns pending token rewards that can be claimed by a specific user using a [`Asset`] object.
    PendingReward { user: String },
}

/// This struct is used to store bLUNA stableswap specific parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolParams {
    /// The current pool amplification
    pub amp: u64,
    /// The bLUNA rewarder contract
    pub bluna_rewarder: String,
    /// The Astroport Generator contract
    pub generator: String,
}

/// This struct is used to store the stableswap pool configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolConfig {
    /// The current pool amplification
    pub amp: Decimal,
    /// The bLUNA rewarder contract
    pub bluna_rewarder: Addr,
    /// The Astroport Generator contract
    pub generator: Addr,
}

/// This enum stores the options available to update bLUNA stableswap pool parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StablePoolUpdateParams {
    StartChangingAmp { next_amp: u64, next_amp_time: u64 },
    StopChangingAmp {},
    BlunaRewarder { address: String },
}

/// This struct contains the parameters used to migrate the bLUNA-LUNA stableswap pool implementation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    /// The bLUNA rewarder contract
    pub bluna_rewarder: Option<String>,
    /// The Astroport Generator contract
    pub generator: Option<String>,
}
