use astroport::asset::AssetInfo;
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, JsonSchema)]
pub struct ExecuteOnReply {
    pub asset_infos: Vec<[AssetInfo; 2]>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const CONVERT_MULTIPLE: Item<ExecuteOnReply> = Item::new("convert_multiple");
