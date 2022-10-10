use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal256, Uint128};
use cw_storage_plus::Item;

/// This structure stores the latest cumulative and average token prices for the target pool
#[cw_serde]
pub struct PriceCumulativeLastV100 {
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

pub const PRICE_LAST_V100: Item<PriceCumulativeLastV100> = Item::new("price_last");
