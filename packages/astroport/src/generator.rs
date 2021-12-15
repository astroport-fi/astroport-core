use cosmwasm_std::{Addr, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// contract address that used for controls settings
    pub owner: String,
    /// ASTRO token contract address
    pub astro_token: String,
    /// tokens per block
    pub tokens_per_block: Uint128,
    /// start block
    pub start_block: Uint64,
    /// allowed reward proxies contracts
    pub allowed_reward_proxies: Vec<String>,
    /// a vesting contract
    pub vesting_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Update current vesting contract
    /// ## Executor
    /// Only owner can execute it
    UpdateConfig {
        /// the vesting contract
        vesting_contract: Option<String>,
    },
    /// ## Description
    /// Add a new liquidity pool token:
    /// ## Executor
    /// Only owner can execute it
    Add {
        /// the LP token contract address
        lp_token: Addr,
        /// the allocation point of liquidity pool
        alloc_point: Uint64,
        /// the reward proxy contract
        reward_proxy: Option<String>,
    },
    /// ## Description
    /// Update the given pool's ASTRO allocation point
    /// ## Executor
    /// Only owner can execute it
    Set {
        /// the LP token contract address
        lp_token: Addr,
        /// the allocation point of liquidity pool
        alloc_point: Uint64,
    },
    /// ## Description
    /// Updates reward variables for all pools
    MassUpdatePools {},
    /// ## Description
    /// Updates reward variables of the given pool to be up-to-date
    UpdatePool {
        /// the LP token contract address
        lp_token: Addr,
    },
    /// ## Description
    /// Withdraw LP tokens from Generator.
    Withdraw {
        /// the LP token contract address
        lp_token: Addr,
        /// the amount of withdrawal
        amount: Uint128,
    },
    /// ## Description
    /// Withdraw LP tokens from Generator without caring about rewards.
    EmergencyWithdraw {
        /// the LP token contract address
        lp_token: Addr,
    },
    /// ## Description
    /// allowed reward proxies contracts
    SetAllowedRewardProxies {
        /// the list of allowed contracts
        proxies: Vec<String>,
    },
    /// ## Description
    /// Sends the orphan proxy rewards which are left by emergency withdrawals
    SendOrphanProxyReward {
        /// the recipient of withdraw
        recipient: String,
        /// the LP token contract address
        lp_token: String,
    },
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ## Description
    /// a new count of tokens per block
    /// ## Executor
    /// Only owner can execute it
    SetTokensPerBlock {
        /// the amount
        amount: Uint128,
    },
    /// ## Description
    /// Creates a request to change ownership
    /// ## Executor
    /// Only owner can execute it
    ProposeNewOwner {
        /// a new ownership
        owner: String,
        /// the validity period of the offer to change the owner
        expires_in: u64,
    },
    /// ## Description
    /// Removes a request to change ownership
    /// ## Executor
    /// Only owner can execute it
    DropOwnershipProposal {},
    /// ## Description
    /// Approves ownership
    /// ## Executor
    /// Only owner can execute it
    ClaimOwnership {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// PoolLength
    PoolLength {},
    /// Deposit
    Deposit { lp_token: Addr, user: Addr },
    /// PendingToken
    PendingToken { lp_token: Addr, user: Addr },
    /// Config returns the base setting of the generator
    Config {},
    /// RewardInfo returns reward information for the specified token.
    RewardInfo { lp_token: Addr },
    /// OrphanProxyRewards returns reward information for the specified token.
    OrphanProxyRewards { lp_token: Addr },
}

/// ## Description
/// This structure describe response for pool length.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolLengthResponse {
    pub length: usize,
}

/// ## Description
/// This structure describes the response to the pending token.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingTokenResponse {
    /// a pending token
    pub pending: Uint128,
    /// a pending token on proxy
    pub pending_on_proxy: Option<Uint128>,
}

/// ## Description
/// This structure describes the response to the reward information.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    /// a base reward token
    pub base_reward_token: Addr,
    /// a proxy reward token
    pub proxy_reward_token: Option<Addr>,
}

/// ## Description
/// This structure describes the response for base controls.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// contract address that used for controls settings
    pub owner: Addr,
    /// ASTRO token contract address
    pub astro_token: Addr,
    /// tokens per block
    pub tokens_per_block: Uint128,
    /// total allocation point
    pub total_alloc_point: Uint64,
    /// start block
    pub start_block: Uint64,
    /// allowed reward proxies
    pub allowed_reward_proxies: Vec<Addr>,
    /// a vesting contract
    pub vesting_contract: Addr,
}

/// ## Description
/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// This structure describes the custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Deposit performs the operation of depositing to the sender.
    Deposit {},
    /// DepositFor performs performs the operation of depositing to the recipient.
    DepositFor(Addr),
}
