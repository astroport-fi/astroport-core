use astroport::native_coin_wrapper::Config;
use cw_storage_plus::Item;

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
