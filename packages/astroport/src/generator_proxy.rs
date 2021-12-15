use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the generator contract address
    pub generator_contract_addr: String,
    /// the pair contract address
    pub pair_addr: String,
    /// the liquidity pool token contract address
    pub lp_token_addr: String,
    /// the reward contract address
    pub reward_contract_addr: String,
    /// the reward token contract address
    pub reward_token_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Deposit {},
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Withdrawal pending rewards
    UpdateRewards {},
    /// Sends rewards to the recipient
    SendRewards { account: Addr, amount: Uint128 },
    /// Withdrawal the rewards
    Withdraw {
        /// the recipient for withdrawal
        account: Addr,
        /// the amount of withdraw
        amount: Uint128,
    },
    /// Withdrawal the rewards
    EmergencyWithdraw {
        /// the recipient for withdrawal
        account: Addr,
        /// the amount of withdraw
        amount: Uint128,
    },
    /// the callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// ## Description
/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    TransferLpTokensAfterWithdraw {
        /// the recipient
        account: Addr,
        /// the previous lp balance for calculate withdraw amount
        prev_lp_balance: Uint128,
    },
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the deposit amount
    Deposit {},
    /// Returns the balance of reward token
    Reward {},
    /// Returns the pending rewards
    PendingToken {},
    /// Returns the reward token contract address
    RewardInfo {},
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
