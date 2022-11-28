use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, Decimal256, Uint128};

use crate::asset::PairInfo;
use crate::asset::{Asset, AssetInfo};
use crate::pair::{
    ConfigResponse, CumulativePricesResponse, PoolResponse, ReverseSimulationResponse,
    SimulationResponse,
};

/// This structure holds concentrated pool parameters.
#[cw_serde]
pub struct ConcentratedPoolParams {
    pub amp: Decimal,
    pub gamma: Decimal,
    pub mid_fee: Decimal,
    pub out_fee: Decimal,
    pub fee_gamma: Decimal,
    pub repeg_profit_threshold: Decimal,
    pub min_price_scale_delta: Decimal,
    /// 1 x\[0] = initial_price_scale * x\[1]
    pub initial_price_scale: Decimal,
    pub ma_half_time: u64,
    pub owner: Option<String>,
}

/// This structure holds concentrated pool parameters which can be changed immediately.
#[cw_serde]
pub struct UpdatePoolParams {
    pub mid_fee: Option<Decimal>,
    pub out_fee: Option<Decimal>,
    pub fee_gamma: Option<Decimal>,
    pub repeg_profit_threshold: Option<Decimal>,
    pub min_price_scale_delta: Option<Decimal>,
    pub ma_half_time: Option<u64>,
}

/// Amp and gamma should be changed gradually. This structure holds all necessary parameters.
#[cw_serde]
pub struct PromoteParams {
    pub next_amp: Decimal,
    pub next_gamma: Decimal,
    pub future_time: u64,
}

/// This enum intended for parameters update.
#[cw_serde]
pub enum ConcentratedPoolUpdateParams {
    Update(UpdatePoolParams),
    Promote(PromoteParams),
    StopChangingAmpGamma {},
}

/// Represents current Amp and Gamma values with future time when change will stop.
#[cw_serde]
pub struct AmpGammaResponse {
    pub amp: Decimal,
    pub gamma: Decimal,
    pub future_time: u64,
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`super::asset::PairInfo`].
    #[returns(PairInfo)]
    Pair {},
    /// Returns information about a pool in an object of type [`PoolResponse`].
    #[returns(PoolResponse)]
    Pool {},
    /// Returns contract configuration settings in a custom [`ConfigResponse`] structure.
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    #[returns(Vec<Asset>)]
    Share { amount: Uint128 },
    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    #[returns(SimulationResponse)]
    Simulation {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
    },
    /// Returns information about cumulative prices in a [`ReverseSimulationResponse`] object.
    #[returns(ReverseSimulationResponse)]
    ReverseSimulation {
        offer_asset_info: Option<AssetInfo>,
        ask_asset: Asset,
    },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    #[returns(CumulativePricesResponse)]
    CumulativePrices {},
    /// Returns current D invariant in as a [`u128`] value
    #[returns(Decimal256)]
    ComputeD {},
    /// Query LP token price denominated in first asset
    #[returns(Decimal256)]
    LpPrice {},
    /// Query current Amp and Gamma
    #[returns(AmpGammaResponse)]
    AmpGamma {},
}
