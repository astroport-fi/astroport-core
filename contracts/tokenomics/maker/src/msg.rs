use astroport::asset::{Asset, AssetInfo};
use cosmwasm_std::{Addr, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub astro_token_contract: String,
    pub factory_contract: String,
    pub staking_contract: String,
    pub governance_contract: String,
    pub governance_percent: Uint64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Collect {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
    SetConfig {
        staking_contract: Option<String>,
        governance_contract: Option<String>,
        governance_percent: Option<Uint64>,
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
    pub astro_token_contract: Addr,
    pub factory_contract: Addr,
    pub staking_contract: Addr,
    pub governance_contract: Addr,
    pub governance_percent: Uint64,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBalancesResponse {
    pub balances: Vec<Asset>,
}
