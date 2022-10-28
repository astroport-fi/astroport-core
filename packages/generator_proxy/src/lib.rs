use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;

/// This structure describes the basic parameters for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The generator contract address
    pub generator_contract_addr: String,
    /// The pair contract address used in this generator proxy
    pub pair_addr: String,
    /// The LP contract address which can be staked in the reward_contract
    pub lp_token_addr: String,
    /// The 3rd party reward contract address
    pub reward_contract_addr: String,
    /// The 3rd party reward token contract address
    pub reward_token_addr: String,
}

#[cw_serde]
pub enum Cw20HookMsg {
    Deposit {},
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Withdraw pending token rewards from the 3rd party staking contract
    UpdateRewards {},
    /// Sends rewards to a recipient
    SendRewards { account: String, amount: Uint128 },
    /// Withdraw LP tokens and outstanding token rewards
    Withdraw {
        /// The address that will receive the withdrawn tokens and rewards
        account: String,
        /// The amount of LP tokens to withdraw
        amount: Uint128,
    },
    /// Withdraw LP tokens without claiming rewards
    EmergencyWithdraw {
        /// The address that will receive the withdrawn tokens
        account: String,
        /// The amount of LP tokens to withdraw
        amount: Uint128,
    },
    /// Callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// This structure describes the callback messages available in the contract.
#[cw_serde]
pub enum CallbackMsg {
    TransferLpTokensAfterWithdraw {
        /// The LP token recipient
        account: Addr,
        /// The previous LP balance for the contract. This is used to calculate
        /// the amount of received LP tokens after withdrawing from a third party contract
        prev_lp_balance: Uint128,
    },
}

/// This structure describes query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the contract's core configuration
    #[returns(ConfigResponse)]
    Config {},
    /// Returns the amount of deposited LP tokens
    #[returns(Uint128)]
    Deposit {},
    /// Returns the amount of rewards to be distributed
    #[returns(Uint128)]
    Reward {},
    /// Returns the amount of pending rewards which can be claimed right now
    #[returns(Uint128)]
    PendingToken {},
    /// Returns the 3rd party reward token contract address
    #[returns(Addr)]
    RewardInfo {},
}

pub type ConfigResponse = InstantiateMsg;

/// This structure describes a migration message.
/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}
