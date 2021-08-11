use astroport::asset::PairInfo;
use cw_storage_plus::Item;

pub const PAIR_INFO: Item<PairInfo> = Item::new("pair_info");
