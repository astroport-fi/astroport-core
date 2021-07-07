use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{AssetInfo, WeightedAssetInfo, WeightedAssetInfoRaw};
use crate::hook::InitHook;
use cosmwasm_std::{
    Api, CanonicalAddr, Extern, HumanAddr,
    Querier, StdResult, Storage,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    /// Pair contract code ID, which is used to
    pub pair_code_id: u64,
    pub token_code_id: u64,
    pub init_hook: Option<InitHook>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    /// UpdateConfig update relevant code IDs
    UpdateConfig {
        owner: Option<HumanAddr>,
        token_code_id: Option<u64>,
        pair_code_id: Option<u64>,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Asset infos
        asset_infos: [WeightedAssetInfo; 2],
        start_time: u64,
        end_time: u64,
        description: Option<String>,
        /// Init hook for after works
        init_hook: Option<InitHook>,
    },
    /// Register is invoked from created pair contract after initialzation
    Register { asset_infos: [WeightedAssetInfo; 2] },
    Unregister { asset_infos: [AssetInfo; 2] },
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
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: HumanAddr,
    pub pair_code_id: u64,
    pub token_code_id: u64,
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairsResponse {
    pub pairs: Vec<FactoryPairInfo>,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct FactoryPairInfo {
    pub asset_infos: [WeightedAssetInfo; 2],
    pub owner: HumanAddr,
    pub contract_addr: HumanAddr,
    pub liquidity_token: HumanAddr,
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct FactoryPairInfoRaw {
    pub asset_infos: [WeightedAssetInfoRaw; 2],
    pub owner: CanonicalAddr,
    pub contract_addr: CanonicalAddr,
    pub liquidity_token: CanonicalAddr,
    pub start_time: u64,
    pub end_time: u64,
}

impl FactoryPairInfoRaw {
    pub fn to_normal<S: Storage, A: Api, Q: Querier>(
        &self,
        deps: &Extern<S, A, Q>,
    ) -> StdResult<FactoryPairInfo> {
        Ok(FactoryPairInfo {
            owner: deps.api.human_address(&self.owner)?,
            liquidity_token: deps.api.human_address(&self.liquidity_token)?,
            start_time: self.start_time,
            contract_addr: deps.api.human_address(&self.contract_addr)?,
            asset_infos: [
                self.asset_infos[0].to_normal(&deps)?,
                self.asset_infos[1].to_normal(&deps)?,
            ],

            end_time: self.end_time,
        })
    }
}
