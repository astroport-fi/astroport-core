use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    /// the token code identifier
    pub token_code_id: u64,
    /// the deposit token address
    pub deposit_token_addr: String,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received
    /// template.
    Receive(Cw20ReceiveMsg),
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns controls settings that specified in custom [`ConfigResponse`] structure
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// the deposit token address
    pub deposit_token_addr: Addr,
    /// the share token address
    pub share_token_addr: Addr,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Deposits token to get share token amount
    Enter {},
    /// Unstakes share token to move back deposit token amount. Burns share.
    Leave {},
}
