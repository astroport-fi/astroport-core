use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetInfo};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// The default swap slippage
pub const DEFAULT_SLIPPAGE: &str = "0.005";
/// The maximum allowed swap slippage
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";

// Decimal precision for TWAP results
pub const TWAP_PRECISION: u8 = 6;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorExecuteMsg {
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    DepositStable {},

    /// Return stable coins to a user
    /// according to exchange rate
    RedeemStable {},

    SetToken(String),

    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorQueryMsg {
    State { block_height: Option<u64> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub total_liabilities: Decimal256,
    pub total_reserves: Decimal256,
    pub last_interest_updated: u64,
    pub last_reward_updated: u64,
    pub global_interest_index: Decimal256,
    pub global_reward_index: Decimal256,
    pub anc_emission_rate: Decimal256,
    pub prev_aterra_supply: Uint256,
    pub prev_exchange_rate: Decimal256,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity allows someone to provide liquidity in the pool
    ProvideLiquidity {
        /// The assets available in the pool
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

    // Intermediate message for sending result
    AssertAndSend {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,

        /// Information about an asset stored in a [`AssetInfo`] struct
        ask_asset_info: AssetInfo,

        /// Receiver who should receive the funds
        receiver: String,

        /// Sender who initiated the transaction
        sender: String,
    },
}

/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Swap a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Withdraw liquidity from the pool
    WithdrawLiquidity {},
}

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`super::asset::PairInfo`].
    Pair {},
    /// Returns information about a pool in an object of type [`PoolResponse`].
    Pool {},
    /// Returns contract configuration settings in a custom [`ConfigResponse`] structure.
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    Share { amount: Uint128 },
    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    Simulation { offer_asset: Asset },
    /// Returns information about cumulative prices in a [`CumulativePricesResponse`] object.
    ReverseSimulation { ask_asset: Asset },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    CumulativePrices {},
}

/// This struct is used to store bLUNA stableswap specific parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AnchorPoolParams {
    /// The anchor contract address
    pub anchor_market_addr: String,
}

/// This struct is used to return a query result with the total amount of LP tokens and the two assets in a specific pool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    /// The assets in the pool together with asset amounts
    pub assets: [Asset; 2],
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
}

/// This struct is used to return a query result with the general contract configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Last timestamp when the cumulative prices in the pool were updated
    pub block_time_last: u64,
    /// The pool's parameters
    pub params: Option<Binary>,
}

/// This structure holds the parameters that are returned from a swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulationResponse {
    /// The amount of ask assets returned by the swap
    pub return_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure holds the parameters that are returned from a reverse swap simulation response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReverseSimulationResponse {
    /// The amount of offer assets returned by the reverse swap
    pub offer_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure is used to return a cumulative prices query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CumulativePricesResponse {
    /// The two assets in the pool to query
    pub assets: [Asset; 2],
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
    /// The last value for the token0 cumulative price
    pub price0_cumulative_last: Uint128,
    /// The last value for the token1 cumulative price
    pub price1_cumulative_last: Uint128,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
