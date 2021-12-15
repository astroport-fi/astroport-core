use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// This structure describes the main controls configs for router
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The factory contract address
    pub astroport_factory: Addr,
}
