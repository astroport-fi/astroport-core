use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{AssetInfo, PairInfo};
use crate::hook::InitHook;
use cosmwasm_std::Addr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Pair contract code IDs which are allowed for pair creation
    pub pair_code_ids: Vec<u64>,
    pub token_code_id: u64,
    pub init_hook: Option<InitHook>,
    // Contract address to send fees to
    pub fee_address: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig update relevant code IDs
    UpdateConfig {
        owner: Option<Addr>,
        token_code_id: Option<u64>,
        pair_code_ids: Option<Vec<u64>>,
        fee_address: Option<Addr>,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// ID of pair contract
        pair_code_id: u64,
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        /// Init hook for after works
        init_hook: Option<InitHook>,
    },
    /// Register is invoked from created pair contract after initialzation
    Register {
        asset_infos: [AssetInfo; 2],
    },
    Deregister {
        asset_infos: [AssetInfo; 2],
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Pairs {
        start_after: Option<[AssetInfo; 2]>,
        limit: Option<u32>,
    },
    FeeAddress {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: Addr,
    pub pair_code_ids: Vec<u64>,
    pub token_code_id: u64,
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairsResponse {
    pub pairs: Vec<PairInfo>,
}
