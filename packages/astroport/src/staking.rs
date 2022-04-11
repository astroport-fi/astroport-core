use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the parameters used for creating a contract.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    /// The contract owner address
    pub owner: String,
    /// CW20 token code identifier
    pub token_code_id: u64,
    /// The ASTRO token contract address
    pub deposit_token_addr: String,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
    Receive(Cw20ReceiveMsg),
}

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns the contract configuration specified in a custom [`ConfigResponse`] structure
    Config {},
    TotalShares {},
    TotalDeposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// The ASTRO token address
    pub deposit_token_addr: Addr,
    /// The xASTRO token address
    pub share_token_addr: Addr,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Deposits ASTRO in exchange for xASTRO
    Enter {},
    /// Burns xASTRO in exchange for ASTRO
    Leave {},
}
