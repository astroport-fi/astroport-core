use schemars::{JsonSchema};
use serde::{Deserialize, Serialize};

use terraswap::vesting::{VestingInfo, OrderBy};
use cosmwasm_std::{CanonicalAddr, StdResult, Timestamp, Addr, Deps};
use cw_storage_plus::{Item, Map, Bound};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub token_addr: CanonicalAddr,
    pub genesis_time: Timestamp,
}

pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");
pub const VESTING_INFO: Map<CanonicalAddr, VestingInfo> = Map::new("vesting_info");

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub fn read_vesting_infos(
    deps: Deps,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(CanonicalAddr, VestingInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (calc_range_start_addr(start_after).map(Bound::exclusive), None, OrderBy::Asc),
        _ => (None, calc_range_end_addr(start_after).map(Bound::exclusive), OrderBy::Desc),
    };

    VESTING_INFO
        .range(deps.storage, start, end, order_by.into())
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start_addr(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let mut v = addr.as_slice().to_vec();
        v.push(1);
        v
    })
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_end_addr(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| addr.as_slice().to_vec())
}
