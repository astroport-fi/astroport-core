use crate::asset::{Asset, AssetInfo};
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure holds concentrated pool parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConcentratedPoolParams {
    pub amp: u128,
    pub gamma: u128,
    pub mid_fee: u128,
    pub out_fee: u128,
    pub fee_gamma: u128,
    // Decimal value with MULTIPLIER denominator, e.g. 100_000_000_000 = 0.0000001
    pub allowed_extra_profit: u128,
    pub adjustment_step: u128,
    pub ma_half_time: u64,
    pub owner: Option<String>,
}

/// This structure holds concentrated pool parameters which can be changed immediately.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdatePoolParams {
    pub mid_fee: Option<u128>,
    pub out_fee: Option<u128>,
    pub fee_gamma: Option<u128>,
    pub allowed_extra_profit: Option<u128>,
    pub adjustment_step: Option<u128>,
    pub ma_half_time: Option<u64>,
}

/// Amp and gamma should be changed gradually. This structure holds all necessary parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PromoteParams {
    pub next_amp: u128,
    pub next_gamma: u128,
    pub future_time: u64,
}

/// This enum intended for parameters update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConcentratedPoolUpdateParams {
    Update(UpdatePoolParams),
    Promote(PromoteParams),
    StopChangingAmpGamma {},
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
    Simulation {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
    },
    /// Returns information about cumulative prices in a [`CumulativePricesResponse`] object.
    ReverseSimulation {
        offer_asset_info: Option<AssetInfo>,
        ask_asset: Asset,
    },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    CumulativePrices {},
    /// Returns current D invariant in as a [`u128`] value
    QueryComputeD {},
    /// Query LP token price denominated in first asset
    LpPrice {},
}
