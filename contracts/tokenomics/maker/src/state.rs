use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use astroport::asset::AssetInfo;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub owner: Addr,
    pub contract: Addr,
    pub factory: Addr,
    pub staking: Addr,
    pub astro_token: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, JsonSchema)]
pub struct  ExecuteOnReply {
    pub token0: Vec<AssetInfo>,
    pub token1: Vec<AssetInfo>,
}

pub const STATE: Item<State> = Item::new("state");
pub const CONVERT_MULTIPLE: Item<ExecuteOnReply> = Item::new("convert_multiple");
