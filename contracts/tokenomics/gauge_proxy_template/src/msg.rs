use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub gauge_contract_addr: String,
    pub lp_token_addr: String,
    pub reward_contract_addr: String,
    pub reward_token_addr: String,
}
