use cosmwasm_std::Uint128;
use cw_storage_plus::Item;

use crate::msg::Config;

pub const CONFIG: Item<Config> = Item::new("config");
pub const TOTAL_BURNED: Item<Uint128> = Item::new("total_burned");
