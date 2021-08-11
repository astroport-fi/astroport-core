use cosmwasm_std::{Addr, Event, Uint128, WasmMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::asset::AssetInfo;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub factory: Addr,
    pub staking: Addr,
    pub astro: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    //SetBridge {token:Addr, bridge:Addr},
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
    //BridgeFor {token: Addr},
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
    pub address: AssetInfo,
}
