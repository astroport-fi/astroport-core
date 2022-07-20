use cosmwasm_std::{Uint128, Uint256};
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
