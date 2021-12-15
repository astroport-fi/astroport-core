use astroport::asset::PairInfo;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of pair stable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the type of pair info available in [`PairInfo`]
    pub pair_info: PairInfo,
    /// the factory contract address
    pub factory_addr: Addr,
    /// The last time block
    pub block_time_last: u64,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    pub init_amp: u64,
    pub init_amp_time: u64,
    pub next_amp: u64,
    pub next_amp_time: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
