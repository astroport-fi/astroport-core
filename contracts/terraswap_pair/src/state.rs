use cosmwasm_std::{StdResult, Storage};

use cosmwasm_storage::{ReadonlySingleton, Singleton};
use terraswap::PairInfoRaw;

static KEY_PAIR_INFO: &[u8] = b"pair_info";

pub fn store_pair_info<S: Storage>(storage: &mut S, data: &PairInfoRaw) -> StdResult<()> {
    Singleton::new(storage, KEY_PAIR_INFO).save(data)
}

pub fn read_pair_info<S: Storage>(storage: &S) -> StdResult<PairInfoRaw> {
    ReadonlySingleton::new(storage, KEY_PAIR_INFO).load()
}
