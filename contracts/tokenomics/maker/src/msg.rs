use astroport::asset::AssetInfo;
use cosmwasm_std::{Addr, Event, Uint128, WasmMsg};
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
    Convert {
        token1: AssetInfo,
        token2: AssetInfo,
    },
    ConvertMultiple {
        token1: Vec<AssetInfo>,
        token2: Vec<AssetInfo>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetFactory {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConvertResponse {
    pub amount: Uint128,
    pub massages: Option<Vec<WasmMsg>>,
    pub events: Option<Vec<Event>>,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryAddressResponse {
    pub address: Addr,
}
