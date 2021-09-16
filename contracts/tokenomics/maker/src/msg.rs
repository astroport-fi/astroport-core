use astroport::asset::{Asset, AssetInfo};
use cosmwasm_std::{Addr, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub factory_contract: String,
    pub staking_contract: String,
    pub governance_contract: String,
    pub governance_percent: Uint64,
    pub astro_token_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Collect {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
    SetConfig {
        governance_percent: Uint64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Balances {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryConfigResponse {
    pub owner: Addr,
    pub factory_contract: Addr,
    pub staking_contract: Addr,
    pub governance_contract: Addr,
    pub governance_percent: Uint64,
    pub astro_token_contract: Addr,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBalancesResponse {
    pub balances: Vec<Asset>,
}
