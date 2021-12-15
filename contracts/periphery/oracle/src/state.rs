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
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    /// The average price 0 asset in pool
    pub price_0_average: Decimal256,
    /// The average price 1 asset in pool
    pub price_1_average: Decimal256,
    /// The last timestamp block in pool
    pub block_timestamp_last: u64,
}

/// ## Description
/// Contract global configuration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The contract address that used for controls settings
    pub owner: Addr,
    /// The factory contract address
    pub factory: Addr,
    /// The assets in the pool. Describes in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// e.g. xyk, stable, etc.
    pub pair: PairInfo,
}
