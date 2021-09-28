use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub generator_contract_addr: Addr,
    pub pair_addr: Addr,
    pub lp_token_addr: Addr,
    pub reward_contract_addr: Addr,
    pub reward_token_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
