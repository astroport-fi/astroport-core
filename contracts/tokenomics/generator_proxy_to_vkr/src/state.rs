use cosmwasm_schema::cw_serde;

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub generator_contract_addr: Addr,
    pub pair_addr: Addr,
    pub lp_token_addr: Addr,
    pub reward_contract_addr: Addr,
    pub reward_token_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
