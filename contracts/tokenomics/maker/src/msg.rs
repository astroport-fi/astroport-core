use astroport::asset::AssetInfo;
use cosmwasm_std::Addr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub factory_contract: String,
    pub staking_contract: String,
    pub astro_token_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Convert { asset_infos: [AssetInfo; 2] },
    ConvertMultiple { asset_infos: Vec<[AssetInfo; 2]> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryConfigResponse {
    pub owner: Addr,
    pub factory_contract: Addr,
    pub staking_contract: Addr,
    pub astro_token_contract: Addr,
}
