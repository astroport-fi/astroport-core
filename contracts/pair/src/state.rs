use astroport::asset::{AssetInfo, PairInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, SnapshotMap};

/// This structure stores the main config parameters for a constant product pair contract.
#[cw_serde]
pub struct Config {
    /// General pair information (e.g pair type)
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract update the asset cumulative prices
    pub block_time_last: u64,
    /// The last cumulative price for asset 0
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price for asset 1
    pub price1_cumulative_last: Uint128,
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
}

/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores asset balances to query them later at any block height
pub const BALANCES: SnapshotMap<&AssetInfo, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    cw_storage_plus::Strategy::EveryBlock,
);
