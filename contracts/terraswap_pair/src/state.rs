use cw_storage_plus::Item;
use terraswap::asset::PairInfo;

pub const PAIR_INFO: Item<PairInfo> = Item::new("pair_info");
