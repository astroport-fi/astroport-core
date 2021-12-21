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
    /// Callback for distributing Bluna reward
    HandleReward {
        previous_reward_balance: Uint128,
        old_user_share: Uint128,
        old_total_share: Uint128,
        user: Addr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolParams {
    pub amp: u64,
    pub bluna_rewarder: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolConfig {
    pub amp: Decimal,
    pub bluna_rewarder: Addr,
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
}
