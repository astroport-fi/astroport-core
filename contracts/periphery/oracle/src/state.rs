use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::asset::{AssetInfo, PairInfo};
use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
pub const PRICE_LAST: Item<PriceCumulativeLast> = Item::new("price_last");

/// ## Description
/// This structure describes the main controls configs of pair
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PriceCumulativeLast {
    /// Sets the last cumulative price by index 0
    pub price0_cumulative_last: Uint128,
    /// Sets the last cumulative price by index 1
    pub price1_cumulative_last: Uint128,
    /// Sets the average price by index 0
    pub price_0_average: Decimal256,
    /// Sets the average price by index 0
    pub price_1_average: Decimal256,
    /// Sets the last timestamp block
    pub block_timestamp_last: u64,
}

/// ## Description
/// Contract global configuration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Sets contract address that used for controls settings
    pub owner: Addr,
    /// Sets factory contract address
    pub factory: Addr,
    /// Sets the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// Sets the type of pair info available in [`PairInfo`]
    pub pair: PairInfo,
}
