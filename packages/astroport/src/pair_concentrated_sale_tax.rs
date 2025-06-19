use crate::pair::FeeShareConfig;
use crate::pair_concentrated::{ConcentratedPoolParams, PromoteParams, UpdatePoolParams};
use crate::pair_xyk_sale_tax::{SaleTaxConfigUpdates, TaxConfigs};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal};

#[cw_serde]
pub struct StoredTaxConfig {
    /// The configs of the sale taxes
    pub tax_configs: TaxConfigs<Addr>,
    /// The address that is allowed to updated the tax configs
    pub tax_config_admin: Addr,
}

/// This enum intended for parameters update.
#[cw_serde]
pub enum ConcentratedPoolUpdateParamsSaleTax {
    /// Allows to update fee parameters as well as repeg_profit_threshold, min_price_scale_delta and EMA interval.
    Update(UpdatePoolParams),
    /// Starts gradual (de/in)crease of Amp or Gamma parameters. Can handle an update of both of them.
    Promote(PromoteParams),
    /// Stops Amp and Gamma update and stores current values.
    StopChangingAmpGamma {},
    /// Enables the sharing of swap fees with an external party.
    EnableFeeShare {
        /// The fee shared with the fee_share_address
        fee_share_bps: u16,
        /// The fee_share_bps is sent to this address on every swap
        fee_share_address: String,
    },
    DisableFeeShare,
    UpdateSaleTax(SaleTaxConfigUpdates),
}

/// This structure holds concentrated pool parameters along with orderbook params.
#[cw_serde]
pub struct ConcentratedPoolParamsSaleTax {
    pub main_params: ConcentratedPoolParams,
    /// The configs of the trade taxes for the pair.
    pub tax_configs: TaxConfigs<String>,
    /// The address that is allowed to updated the tax configs.
    pub tax_config_admin: String,
}

/// This structure stores a CL pool's configuration.
#[cw_serde]
pub struct ConcentratedPoolConfigSaleTax {
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

    /// The configs of the trade taxes for the pair.
    pub tax_configs: TaxConfigs<String>,
    /// The address that is allowed to updated the tax configs.
    pub tax_config_admin: String,
}
