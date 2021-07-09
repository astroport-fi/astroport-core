use cosmwasm_std::{Addr, Binary, StdError, StdResult, Uint128};
use cw20::{Cw20Coin, Expiration, MinterResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub token_code_id: u64,
    pub astroport_token_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    PostInitialize {},
    Enter {
        amount: Uint128,
    },
    Leave {
        share: Uint128,
    },
}