use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub factory_contract: Addr,
    pub staking_contract: Addr,
    pub astro_token_contract: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
