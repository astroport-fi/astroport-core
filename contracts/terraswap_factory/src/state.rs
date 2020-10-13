use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, ReadonlyStorage, StdError, StdResult, Storage,
};

use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage, ReadonlySingleton, Singleton};
use terraswap::{AssetInfoRaw, PairInfoRaw};

static KEY_CONFIG: &[u8] = b"config";
static PREFIX_PAIR: &[u8] = b"pair";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub pair_code_id: u64,
    pub token_code_id: u64,
}

pub fn store_config<S: Storage>(storage: &mut S, data: &Config) -> StdResult<()> {
    Singleton::new(storage, KEY_CONFIG).save(data)
}

pub fn read_config<S: Storage>(storage: &S) -> StdResult<Config> {
    ReadonlySingleton::new(storage, KEY_CONFIG).load()
}

pub fn store_pair<S: Storage>(storage: &mut S, data: &PairInfoRaw) -> StdResult<()> {
    let mut asset_infos = data.asset_infos.clone().to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(&b.as_bytes()));

    PrefixedStorage::new(PREFIX_PAIR, storage).set(
        &[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat(),
        &to_vec(data)?,
    );

    Ok(())
}

pub fn remove_pair<S: Storage>(storage: &mut S, asset_infos: &[AssetInfoRaw; 2]) {
    let mut asset_infos = asset_infos.clone().to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(&b.as_bytes()));

    PrefixedStorage::new(PREFIX_PAIR, storage)
        .remove(&[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat());
}

pub fn read_pair<S: Storage>(
    storage: &S,
    asset_infos: &[AssetInfoRaw; 2],
) -> StdResult<PairInfoRaw> {
    let mut asset_infos = asset_infos.clone().to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(&b.as_bytes()));

    match ReadonlyPrefixedStorage::new(PREFIX_PAIR, storage)
        .get(&[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat())
    {
        Some(v) => from_slice(&v),
        None => Err(StdError::generic_err("no pair data stored")),
    }
}
