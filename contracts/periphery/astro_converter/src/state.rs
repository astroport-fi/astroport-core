use cw_storage_plus::Item;

use astroport::astro_converter::Config;

pub const CONFIG: Item<Config> = Item::new("config");
