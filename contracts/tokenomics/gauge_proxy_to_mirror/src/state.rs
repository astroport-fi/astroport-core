use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub gauge_contract_addr: Addr,
    pub lp_token_addr: Addr,
    pub reward_contract_addr: Addr,
    pub reward_token_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteOnReply {
    Deposit { account: Addr, amount: Uint128 },
    Withdraw { account: Addr, amount: Uint128 },
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const TMP_USER_ACTION: Item<ExecuteOnReply> = Item::new("tmp_user_action");
