use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetInfo};

use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// the default slippage
pub const DEFAULT_SLIPPAGE: &str = "0.005";
/// the maximum allowed slippage
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";

pub const TWAP_PRECISION: u8 = 6;

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// the token contract code id for initialization
    pub token_code_id: u64,
    /// the factory contract address
    pub factory_addr: Addr,
    /// the optional binary serialised parameters for custom pool types
    pub init_params: Option<Binary>,
}

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
}

/// ## Description
/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Sell a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Withdrawing liquidity from the pool
    WithdrawLiquidity {},
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
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// the last time block
    pub block_time_last: u64,
    pub params: Option<Binary>,
}

/// ## Description
/// SimulationResponse returns swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulationResponse {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// ## Description
/// ReverseSimulationResponse returns reverse swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReverseSimulationResponse {
    pub offer_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CumulativePricesResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
    pub price0_cumulative_last: Uint128,
    pub price1_cumulative_last: Uint128,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolParams {
    pub amp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StablePoolConfig {
    pub amp: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StablePoolUpdateParams {
    StartChangingAmp { next_amp: u64, next_amp_time: u64 },
    StopChangingAmp {},
}
