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
}

/// This struct is used to store bLUNA lido specific parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct LidoPoolParams {
    /// The Lido contract addresses
    pub hub_address: String,
    pub stluna_addr: String,
    pub bluna_addr: String,
}

/// This struct is used to store the lido pool configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    pub hub_address: Addr,
    pub stluna_address: Addr,
    pub bluna_address: Addr,
    pub block_time_last: u64,
}
