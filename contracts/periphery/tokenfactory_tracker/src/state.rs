use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};

#[cw_serde]
pub struct Config {
    pub tracked_denom: String,
    pub tokenfactory_module_address: String,
}

pub const CONFIG: Item<Config> = Item::new("config");

/// Contains snapshotted balances at every block.
pub const BALANCES: SnapshotMap<&String, Uint128> = SnapshotMap::new(
    "balance",
    "balance__checkpoints",
    "balance__changelog",
    Strategy::EveryBlock,
);

/// Contains the history of the total supply of the tracked denom
pub const TOTAL_SUPPLY_HISTORY: Map<u64, Uint128> = Map::new("total_supply_history");
