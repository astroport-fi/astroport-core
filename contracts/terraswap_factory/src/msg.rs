use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, HumanAddr};
use terraswap::{AssetInfo, InitHook};

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
        /// Pair contract owner
        pair_owner: HumanAddr,
        /// Inactive commission collector
        commission_collector: HumanAddr,
        /// Commission rate for active liquidity provider
        lp_commission: Decimal,
        /// Commission rate for owner controlled commission
        owner_commission: Decimal,
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        /// Init hook for after works
        init_hook: Option<InitHook>,
    },
    /// Register is invoked from created pair contract after initialzation
    Register { asset_infos: [AssetInfo; 2] },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Pair { asset_infos: [AssetInfo; 2] },
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
