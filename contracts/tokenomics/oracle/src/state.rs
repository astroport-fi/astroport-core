use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource<A> {
    Native { denom: String },
    TerraswapUusdPair { pair_address: A },
    Fixed { price: Decimal },
}

pub type PriceSourceUnchecked = PriceSource<String>;
pub type PriceSourceChecked = PriceSource<Addr>;

/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
pub const PRICE_CONFIGS: Map<&[u8], PriceConfig> = Map::new("price_configs");

/// Contract global configuration
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
}

/// Price source configuration for a given asset info
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PriceConfig {
    pub price_source: PriceSourceChecked,
}
