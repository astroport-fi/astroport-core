use astroport::pair_bonded::Config;
use cw_storage_plus::Item;

/// ## Description
/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");
