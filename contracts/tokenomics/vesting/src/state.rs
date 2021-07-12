use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use terraswap::vesting::{VestingInfo, OrderBy};
use cosmwasm_std::{CanonicalAddr, ReadonlyStorage, StdResult, Storage};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, ReadonlyBucket};

const KEY_CONFIG: &[u8] = b"config";
const PREFIX_KEY_VESTING_INFO: &[u8] = b"vesting_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub token_addr: CanonicalAddr,
    pub genesis_time: u64,
}

pub fn store_config<S: Storage>(storage: &mut S, config: &Config) -> StdResult<()> {
    Ok(singleton::<S, Config>(storage, KEY_CONFIG).save(&config)?)
}

pub fn read_config<S: Storage>(storage: &S) -> StdResult<Config> {
    Ok(singleton_read::<S, Config>(storage, KEY_CONFIG).load()?)
}

pub fn read_vesting_info<S: ReadonlyStorage>(
    storage: &S,
    address: &CanonicalAddr,
) -> StdResult<VestingInfo> {
    Ok(bucket_read::<S, VestingInfo>(PREFIX_KEY_VESTING_INFO, storage).load(address.as_slice())?)
}

pub fn store_vesting_info<S: Storage>(
    storage: &mut S,
    address: &CanonicalAddr,
    vesting_info: &VestingInfo,
) -> StdResult<()> {
    Ok(bucket::<S, VestingInfo>(PREFIX_KEY_VESTING_INFO, storage)
        .save(address.as_slice(), vesting_info)?)
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_vesting_infos<'a, S: ReadonlyStorage>(
    storage: &'a S,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(CanonicalAddr, VestingInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (calc_range_start_addr(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end_addr(start_after), OrderBy::Desc),
    };

    let vesting_accounts: ReadonlyBucket<'a, S, VestingInfo> =
        ReadonlyBucket::new(PREFIX_KEY_VESTING_INFO, storage);

    vesting_accounts
        .range(start.as_deref(), end.as_deref(), order_by.into())
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
