use cosmwasm_schema::cw_serde;

use astroport::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{Addr, Decimal256, Uint128};
use cw_storage_plus::Item;

/// ## Description
/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores the latest cumulative and average prices at the given key
pub const PRICE_LAST: Item<PriceCumulativeLast> = Item::new("price_last");

/// ## Description
/// This structure stores the latest cumulative and average token prices for the target pool
#[cw_serde]
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
/// Global configuration for the contract
#[cw_serde]
pub struct Config {
    /// The address that's allowed to change contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory: Addr,
    /// The assets in the pool. Each asset is described using a [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// Information about the pair (LP token address, pair type etc)
    pub pair: PairInfo,
}
