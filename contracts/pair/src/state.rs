use astroport::asset::PairInfo;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Sets the type of pair info available in [`PairInfo`]
    pub pair_info: PairInfo,
    pub factory_addr: Addr,
    pub block_time_last: u64,
    pub price0_cumulative_last: Uint128,
    pub price1_cumulative_last: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
