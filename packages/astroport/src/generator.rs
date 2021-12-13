use cosmwasm_std::{Addr, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Sets contract address that used for controls settings
    pub owner: String,
    /// Sets CW20 token contract address TODO:
    pub astro_token: String,
    /// Sets tokens per block
    pub tokens_per_block: Uint128,
    /// Sets start block
    pub start_block: Uint64,
    /// Sets allowed reward proxies TODO:
    pub allowed_reward_proxies: Vec<String>,
    /// Sets a vesting contract
    pub vesting_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig update current vesting contract
    UpdateConfig {
        vesting_contract: Option<String>,
    },
    /// Add TODO:
    Add {
        lp_token: Addr,
        alloc_point: Uint64,
        reward_proxy: Option<String>,
    },
    /// Set TODO:
    Set {
        lp_token: Addr,
        alloc_point: Uint64,
    },
    /// MassUpdatePools
    MassUpdatePools {},
    /// UpdatePool
    UpdatePool {
        lp_token: Addr,
    },
    Withdraw {
        lp_token: Addr,
        amount: Uint128,
    },
    EmergencyWithdraw {
        lp_token: Addr,
    },
    SetAllowedRewardProxies {
        proxies: Vec<String>,
    },
    SendOrphanProxyReward {
        recipient: String,
        lp_token: String,
    },
    Receive(Cw20ReceiveMsg),
    SetTokensPerBlock {
        amount: Uint128,
    },
    ProposeNewOwner {
        owner: String,
        expires_in: u64,
    },
    DropOwnershipProposal {},
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
    /// Sets a pending token
    pub pending: Uint128,
    /// Sets a pending token on proxy
    pub pending_on_proxy: Option<Uint128>,
}

/// ## Description
/// This structure describes the response to the reward information.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    /// Sets a base reward token
    pub base_reward_token: Addr,
    /// Sets a proxy reward token
    pub proxy_reward_token: Option<Addr>,
}

/// ## Description
/// This structure describes the response for base controls.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Sets contract address that used for controls settings
    pub owner: Addr,
    /// Sets CW20 token contract address TODO:
    pub astro_token: Addr,
    /// Sets tokens per block
    pub tokens_per_block: Uint128,
    /// Sets total allocation point
    pub total_alloc_point: Uint64,
    /// Sets start block
    pub start_block: Uint64,
    /// Sets allowed reward proxies
    pub allowed_reward_proxies: Vec<Addr>,
    /// Sets a vesting contract
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
