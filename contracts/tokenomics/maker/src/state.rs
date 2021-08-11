use cosmwasm_std::{Addr};
use cw_storage_plus::{Item};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub owner: Addr,
    pub contract: Addr,
    pub factory: Addr,
    pub staking: Addr,
    pub astro_token: Addr,
}

pub const STATE: Item<State> = Item::new("state");
