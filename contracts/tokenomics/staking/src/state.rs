use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub deposit_token_addr: Addr,
    pub share_token_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
