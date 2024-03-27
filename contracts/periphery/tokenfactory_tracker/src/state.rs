use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, SnapshotItem, SnapshotMap, Strategy};

#[cw_serde]
pub struct Config {
    /// Tracked denom
    pub d: String,
    /// Token factory module address
    pub m: String,
}

pub const CONFIG: Item<Config> = Item::new("c");

/// Contains snapshotted balances at every block.
pub const BALANCES: SnapshotMap<&str, Uint128> =
    SnapshotMap::new("b", "b_chpts", "b_chlg", Strategy::EveryBlock);

/// Contains the history of the total supply of the tracked denom
pub const TOTAL_SUPPLY_HISTORY: SnapshotItem<Uint128> =
    SnapshotItem::new("t", "t_chpts", "t_chlg", Strategy::EveryBlock);
