use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, StdError, Uint128};

use crate::pair_concentrated::ConcentratedPoolParams;

#[cw_serde]
pub struct OrderbookConfig {
    /// The address of the orderbook sync executor. If None, then the sync is permissionless.
    pub executor: Option<String>,
    /// Number of orders on each side of the orderbook
    pub orders_number: u8,
    /// Minimum order size for asset 0
    pub min_asset_0_order_size: Uint128,
    /// Minimum order size for asset 1
    pub min_asset_1_order_size: Uint128,
    /// Percent of liquidity to be deployed to the orderbook
    pub liquidity_percent: Decimal,
    /// Due to possible rounding issues on Duality side we have to set price tolerance,
    /// which serves as a worsening factor for the end price from PCL.
    /// Should be relatively low something like 1-10 bps.
    pub avg_price_adjustment: Decimal,
}

#[cw_serde]
pub struct UpdateDualityOrderbook {
    /// Determines whether the orderbook is enabled
    pub enable: Option<bool>,
    /// The address of the orderbook sync executor
    pub executor: Option<String>,
    /// Determines whether the executor should be removed.
    /// If removed, then sync endpoint becomes permissionless
    #[serde(default)]
    pub remove_executor: bool,
    /// Number of orders on each side of the orderbook
    pub orders_number: Option<u8>,
    /// Minimum order size for asset 0
    pub min_asset_0_order_size: Option<Uint128>,
    /// Minimum order size for asset 1
    pub min_asset_1_order_size: Option<Uint128>,
    /// Percent of liquidity to be deployed to the orderbook
    pub liquidity_percent: Option<Decimal>,
    /// Due to possible rounding issues on Duality side we have to set price tolerance,
    /// which serves as a worsening factor for the end price from PCL.
    /// Should be relatively low something like 1-10 bps.
    pub avg_price_adjustment: Option<Decimal>,
}

/// This structure holds concentrated pool parameters along with orderbook params.
#[cw_serde]
pub struct ConcentratedDualityParams {
    pub main_params: ConcentratedPoolParams,
    pub orderbook_config: OrderbookConfig,
}

#[cw_serde]
pub enum DualityPairMsg {
    SyncOrderbook {},
    UpdateOrderbookConfig(UpdateDualityOrderbook),
}

/// A `reply` call code ID used for sub-messages.
#[cw_serde]
pub enum ReplyIds {
    CreateDenom = 1,
    PostLimitOrderCb = 2,
}

impl TryFrom<u64> for ReplyIds {
    type Error = StdError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ReplyIds::CreateDenom),
            2 => Ok(ReplyIds::PostLimitOrderCb),
            _ => Err(StdError::ParseErr {
                target_type: "ReplyIds".to_string(),
                msg: "Failed to parse reply".to_string(),
            }),
        }
    }
}

#[cw_serde]
pub enum MigrateMsg {
    /// Migration from plain PCL to PCL with Duality integration
    MigrateToOrderbook { orderbook_config: OrderbookConfig },
    /// General migration for `astroport-pair-concentrated-duality` pool
    Migrate {},
}
