use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

use astroport::asset::Asset;

#[cw_serde]
pub struct Config {
    pub factory_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub enum ActionParams {
    Provide {
        lp_token_addr: String,
        lp_amount_before: Uint128,
        staked_in_generator: bool,
        min_lp_to_receive: Uint128,
    },
    Withdraw {
        pair_addr: Addr,
        min_assets_to_receive: Vec<Asset>,
    },
}

#[cw_serde]
pub struct ReplyData {
    pub receiver: String,
    pub params: ActionParams,
}

pub const REPLY_DATA: Item<ReplyData> = Item::new("reply_data");
