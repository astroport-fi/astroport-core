use cosmwasm_schema::cw_serde;
use cw_storage_plus::Item;

use astroport::staking::{Config, TrackerData};

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the tracker contract instantiate data at the given key
pub const TRACKER_DATA: Item<TrackerData> = Item::new("tracker_data");

/// What the staking contract lets users do. Only a migration changes it.
#[cw_serde]
#[derive(Default)]
pub enum StakingMode {
    /// Enter and leave both work
    #[default]
    Open,
    /// Only leave works: no new ASTRO can be staked
    LeaveOnly,
    /// Neither enter nor leave works
    Paused,
}

/// The current mode. Unset means [`StakingMode::Open`].
pub const MODE: Item<StakingMode> = Item::new("mode");
