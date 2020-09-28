use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, Api, Decimal, Extern, HumanAddr, Querier, ReadonlyStorage,
    StdError, StdResult, Storage,
};

use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};
use terraswap::{Asset, AssetInfo, AssetInfoRaw, PairConfigRaw};

static PREFIX_CONFIG: &[u8] = b"config";

static KEY_ASSET: &[u8] = b"asset";
static KEY_GENERAL: &[u8] = b"general";
static KEY_SWAP: &[u8] = b"swap";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigSwap {
    pub lp_commission: Decimal,
    pub owner_commission: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigAsset {
    pub assets: [AssetInfoRaw; 2],
}

impl ConfigAsset {
    pub fn to_pools<S: Storage, A: Api, Q: Querier>(
        self: &Self,
        deps: &Extern<S, A, Q>,
        contract_addr: &HumanAddr,
    ) -> StdResult<[Asset; 2]> {
        let info_0: AssetInfo = self.assets[0].to_normal(deps)?;
        let info_1: AssetInfo = self.assets[1].to_normal(deps)?;
        Ok([
            Asset {
                amount: info_0.load_pool(deps, contract_addr)?,
                info: info_0,
            },
            Asset {
                amount: info_1.load_pool(deps, contract_addr)?,
                info: info_1,
            },
        ])
    }
}

pub fn store_config_general<S: Storage>(storage: &mut S, data: &PairConfigRaw) -> StdResult<()> {
    PrefixedStorage::new(PREFIX_CONFIG, storage).set(KEY_GENERAL, &to_vec(data)?);
    Ok(())
}

pub fn read_config_general<S: Storage>(storage: &S) -> StdResult<PairConfigRaw> {
    let data = ReadonlyPrefixedStorage::new(PREFIX_CONFIG, storage).get(KEY_GENERAL);
    match data {
        Some(v) => from_slice(&v),
        None => Err(StdError::generic_err("no general config data stored")),
    }
}

pub fn store_config_swap<S: Storage>(storage: &mut S, data: &ConfigSwap) -> StdResult<()> {
    PrefixedStorage::new(PREFIX_CONFIG, storage).set(KEY_SWAP, &to_vec(data)?);
    Ok(())
}

pub fn read_config_swap<S: Storage>(storage: &S) -> StdResult<ConfigSwap> {
    let data = ReadonlyPrefixedStorage::new(PREFIX_CONFIG, storage).get(KEY_SWAP);
    match data {
        Some(v) => from_slice(&v),
        None => Err(StdError::generic_err("no general swap data stored")),
    }
}

pub fn store_config_asset<S: Storage>(storage: &mut S, data: &ConfigAsset) -> StdResult<()> {
    PrefixedStorage::new(PREFIX_CONFIG, storage).set(KEY_ASSET, &to_vec(data)?);
    Ok(())
}

pub fn read_config_asset<S: Storage>(storage: &S) -> StdResult<ConfigAsset> {
    let data = ReadonlyPrefixedStorage::new(PREFIX_CONFIG, storage).get(KEY_ASSET);
    match data {
        Some(v) => from_slice(&v),
        None => Err(StdError::generic_err("no asset config data stored")),
    }
}
