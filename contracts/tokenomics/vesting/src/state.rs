use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::vesting::{OrderBy, VestingInfo};
use cosmwasm_std::{Addr, CanonicalAddr, Deps, StdResult, Timestamp};
use cw_storage_plus::{Bound, Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub token_addr: CanonicalAddr,
    pub genesis_time: Timestamp,
}

pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");
pub const VESTING_INFO: Map<String, VestingInfo> = Map::new("vesting_info");

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub fn read_vesting_infos(
    deps: Deps,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(Addr, VestingInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (
            calc_range_start_addr(start_after).map(Bound::exclusive),
            None,
            OrderBy::Asc,
        ),
        _ => (
            None,
            calc_range_end_addr(start_after).map(Bound::exclusive),
            OrderBy::Desc,
        ),
    };

    let info: Vec<(Addr, VestingInfo)> = VESTING_INFO
        .range(deps.storage, start, end, order_by.into())
        .take(limit)
        .map(|item| {
            let (k, v) = item.unwrap();
            let addr = deps
                .api
                .addr_validate(String::from_utf8(k).unwrap().as_str())
                .unwrap();
            (addr, v)
        })
        .collect();

    Ok(info)
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
