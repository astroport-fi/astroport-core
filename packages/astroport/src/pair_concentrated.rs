use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, Decimal256, Uint128, Uint64};

use crate::asset::PairInfo;
use crate::asset::{Asset, AssetInfo};
use crate::observation::OracleObservation;
use crate::pair::{
    ConfigResponse, CumulativePricesResponse, FeeShareConfig, PoolResponse,
    ReverseSimulationResponse, SimulationResponse,
};

/// This structure holds concentrated pool parameters.
#[cw_serde]
pub struct ConcentratedPoolParams {
    /// Amplification coefficient affects trades close to price_scale
    pub amp: Decimal,
    /// Affects how gradual the curve changes from constant sum to constant product
    /// as price moves away from price scale. Low values mean more gradual.
    pub gamma: Decimal,
    /// The minimum fee, charged when pool is fully balanced
    pub mid_fee: Decimal,
    /// The maximum fee, charged when pool is imbalanced
    pub out_fee: Decimal,
    /// Parameter that defines how gradual the fee changes from fee_mid to fee_out
    /// based on distance from price_scale.
    pub fee_gamma: Decimal,
    /// Minimum profit before initiating a new repeg
    pub repeg_profit_threshold: Decimal,
    /// Minimum amount to change price_scale when repegging.
    pub min_price_scale_delta: Decimal,
    /// 1 x\[0] = price_scale * x\[1].
    pub price_scale: Decimal,
    /// Half-time used for calculating the price oracle.
    pub ma_half_time: u64,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: Option<bool>,
    /// The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
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
    /// Allows to update fee parameters as well as repeg_profit_threshold, min_price_scale_delta and EMA interval.
    Update(UpdatePoolParams),
    /// Starts gradual (de/in)crease of Amp or Gamma parameters. Can handle an update of both of them.
    Promote(PromoteParams),
    /// Stops Amp and Gamma update and stores current values.
    StopChangingAmpGamma {},
    /// Enable asset balances tracking
    EnableAssetBalancesTracking {},
    /// Enables the sharing of swap fees with an external party.
    EnableFeeShare {
        /// The fee shared with the fee_share_address
        fee_share_bps: u16,
        /// The fee_share_bps is sent to this address on every swap
        fee_share_address: String,
    },
    DisableFeeShare,
}

/// This structure stores a CL pool's configuration.
#[cw_serde]
pub struct ConcentratedPoolConfig {
    /// Amplification coefficient affects trades close to price_scale
    pub amp: Decimal,
    /// Affects how gradual the curve changes from constant sum to constant product
    /// as price moves away from price scale. Low values mean more gradual.
    pub gamma: Decimal,
    /// The minimum fee, charged when pool is fully balanced
    pub mid_fee: Decimal,
    /// The maximum fee, charged when pool is imbalanced
    pub out_fee: Decimal,
    /// Parameter that defines how gradual the fee changes from fee_mid to fee_out
    /// based on distance from price_scale.
    pub fee_gamma: Decimal,
    /// Minimum profit before initiating a new repeg
    pub repeg_profit_threshold: Decimal,
    /// Minimum amount to change price_scale when repegging.
    pub min_price_scale_delta: Decimal,
    /// 1 x\[0] = price_scale * x\[1].
    pub price_scale: Decimal,
    /// Half-time used for calculating the price oracle.
    pub ma_half_time: u64,
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
    /// The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns information about a pair
    #[returns(PairInfo)]
    Pair {},
    /// Returns information about a pool
    #[returns(PoolResponse)]
    Pool {},
    /// Returns contract configuration
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    #[returns(Vec<Asset>)]
    Share { amount: Uint128 },
    /// Returns information about a swap simulation
    #[returns(SimulationResponse)]
    Simulation {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
    },
    /// Returns information about a reverse swap simulation
    #[returns(ReverseSimulationResponse)]
    ReverseSimulation {
        offer_asset_info: Option<AssetInfo>,
        ask_asset: Asset,
    },
    /// Returns information about the cumulative prices
    #[returns(CumulativePricesResponse)]
    CumulativePrices {},
    /// Returns current D invariant
    #[returns(Decimal256)]
    ComputeD {},
    /// Query LP token virtual price
    #[returns(Decimal256)]
    LpPrice {},
    /// Returns the balance of the specified asset that was in the pool just preceding the moment
    /// of the specified block height creation.
    #[returns(Option<Uint128>)]
    AssetBalanceAt {
        asset_info: AssetInfo,
        block_height: Uint64,
    },
    /// Query price from observations
    #[returns(OracleObservation)]
    Observe { seconds_ago: u64 },
}

#[cw_serde]
pub struct MigrateMsg {}
