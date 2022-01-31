use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::Asset;

use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity a user provides pool liquidity
    ProvideLiquidity {
        /// the type of asset available in [`Asset`]
        assets: [Asset; 2],
        /// the slippage tolerance for sets the maximum percent of price movement
        slippage_tolerance: Option<Decimal>,
        /// Determines whether an autostake will be performed on the generator
        auto_stake: Option<bool>,
        /// the receiver of provide liquidity
        receiver: Option<String>,
    },
    /// Swap an offer asset to the other
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Update pair config if required
    UpdateConfig { params: Binary },
    /// Claims the Bluna reward and sends it to the receiver
    ClaimReward {
        /// An address which will receive the reward
        receiver: Option<String>,
    },
    /// Claims the Bluna reward on changing of user lp token amount deposited to the generator
    ClaimRewardByGenerator {
        /// The user whose lp token amount is changing
        user: String,
        /// The user's lp token amount before the change
        user_share: Uint128,
        /// The total lp token amount deposited to the generator before the change
        total_share: Uint128,
    },
    /// Callback for distributing Bluna reward
    HandleReward {
        previous_reward_balance: Uint128,
        user: Addr,
        user_share: Uint128,
        total_share: Uint128,
        receiver: Option<Addr>,
    },
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`PairInfo`].
    Pair {},
    /// Returns information about a pool in an object of type [`PoolResponse`].
    Pool {},
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    Share { amount: Uint128 },
    /// Returns information about the simulation of the swap in a [`SimulationResponse`] object.
    Simulation { offer_asset: Asset },
    /// Returns information about the reverse simulation in a [`ReverseSimulationResponse`] object.
    ReverseSimulation { ask_asset: Asset },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    CumulativePrices {},
    /// Returns pending reward for a user in a [`PendingRewardResponse`] object
    PendingReward { user: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolParams {
    pub amp: u64,
    pub bluna_rewarder: String,
    pub generator: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolConfig {
    pub amp: Decimal,
    pub bluna_rewarder: Addr,
    pub generator: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StablePoolUpdateParams {
    StartChangingAmp { next_amp: u64, next_amp_time: u64 },
    StopChangingAmp {},
    BlunaRewarder { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub bluna_rewarder: String,
    pub generator: String,
}
