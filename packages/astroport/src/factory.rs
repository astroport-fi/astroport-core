use crate::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{to_binary, Addr, DepsMut, QueryRequest, StdResult, WasmQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairType {
    Xyk {},
    Stable {},
}

// Provide a string version of this to raw encode strings
impl Display for PairType {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            PairType::Xyk {} => fmt.write_str("xyk"),
            PairType::Stable {} => fmt.write_str("stable"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairConfig {
    pub code_id: u64,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
}

impl PairConfig {
    pub fn valid_fee_bps(&self) -> bool {
        self.total_fee_bps <= 10_000 && self.maker_fee_bps <= 10_000
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Pair contract code IDs which are allowed to create pairs
    pub pair_xyk_config: Option<PairConfig>,
    pub pair_stable_config: Option<PairConfig>,
    pub token_code_id: u64,
    // Contract address to send fees to
    pub fee_address: Option<Addr>,
    pub generator_address: Addr,
    pub gov: Option<Addr>,
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        gov: Option<UpdateAddr>,
        owner: Option<Addr>,
        token_code_id: Option<u64>,
        fee_address: Option<Addr>,
        generator_address: Option<Addr>,
        pair_xyk_config: Option<PairConfig>,
        pair_stable_config: Option<PairConfig>,
    },
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Asset infos
        asset_infos: [AssetInfo; 2],
    },
    CreatePairStable {
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        /// Amplification point
        amp: u64,
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
    FeeInfo {
        pair_type: PairType,
    },
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: Addr,
    pub gov: Option<Addr>,
    pub pair_xyk_config: Option<PairConfig>,
    pub pair_stable_config: Option<PairConfig>,
    pub token_code_id: u64,
    pub fee_address: Option<Addr>,
    pub generator_address: Addr,
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairsResponse {
    pub pairs: Vec<PairInfo>,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct FeeInfoResponse {
    pub fee_address: Option<Addr>,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
}

/// External query factory config
pub fn factory_config(factory_addr: Addr, deps: &DepsMut) -> StdResult<ConfigResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_addr.to_string(),
        msg: to_binary(&QueryMsg::Config {})?,
    }))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAddr {
    Set { address: String },
    Remove {},
}
