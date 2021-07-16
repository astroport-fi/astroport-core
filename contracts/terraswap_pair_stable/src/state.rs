use cosmwasm_std::{StdResult, Storage};

use cosmwasm_storage::{ReadonlySingleton, Singleton};
use terraswap::asset::PairInfoRaw;

static KEY_PAIR_INFO: &[u8] = b"pair_info";

pub fn store_pair_info(storage: &mut dyn Storage, data: &PairInfoRaw) -> StdResult<()> {
    Singleton::new(storage, KEY_PAIR_INFO).save(data)
}

pub fn read_pair_info(storage: &dyn Storage) -> StdResult<PairInfoRaw> {
    ReadonlySingleton::new(storage, KEY_PAIR_INFO).load()
}
