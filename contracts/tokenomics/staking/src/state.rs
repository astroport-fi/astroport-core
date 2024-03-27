use cw_storage_plus::Item;

use astroport::staking::{Config, TrackerData};

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the tracker contract instantiate data at the given key
pub const TRACKER_DATA: Item<TrackerData> = Item::new("tracker_data");
