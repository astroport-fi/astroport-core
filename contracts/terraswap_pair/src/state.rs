use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::asset::PairInfo;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub pair_info: PairInfo,
    pub k_last: Uint128,
    pub factory_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
