use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Binary, HumanAddr};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitHook {
    pub msg: Binary,
    pub contract_addr: HumanAddr,
}
