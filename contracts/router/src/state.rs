use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// This structure holds the main parameters for the router
#[cw_serde]
pub struct Config {
    /// The factory contract address
    pub astroport_factory: Addr,
}

pub const REPLY_DATA: Item<ReplyData> = Item::new("reply_data");

#[cw_serde]
pub struct ReplyData {
    pub asset_info: AssetInfo,
    pub prev_balance: Uint128,
    pub minimum_receive: Option<Uint128>,
    pub receiver: String,
}
