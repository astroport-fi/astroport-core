use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::Item;

/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
pub const PRICE_LAST: Item<PriceCumulativeLast> = Item::new("price_last");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PriceCumulativeLast {
    pub price0_cumulative_last: Uint128,
    pub price1_cumulative_last: Uint128,
    pub price_0_average: Decimal,
    pub price_1_average: Decimal,
    pub block_timestamp_last: u64,
}

/// Contract global configuration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub factory: Addr,
    pub asset_infos: [AssetInfo; 2],
    pub pair: PairInfo,
}
