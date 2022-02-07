use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the parameters used for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address that can change contract settings
    pub owner: String,
    /// ASTRO token contract address
    pub astro_token: String,
    /// Amount of ASTRO distributed per block among all pairs
    pub tokens_per_block: Uint128,
    /// Start block for distributing ASTRO
    pub start_block: Uint64,
    /// Dual rewards proxy contracts allowed to interact with the generator
    pub allowed_reward_proxies: Vec<String>,
    /// The ASTRO vesting contract that drips ASTRO rewards
    pub vesting_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Update the address of the ASTRO vesting contract
    /// ## Executor
    /// Only the owner can execute it
    UpdateConfig {
        /// The new vesting contract address
        vesting_contract: Option<String>,
    },
    /// ## Description
    /// Add a new generator for a LP token
    /// ## Executor
    /// Only the owner can execute this
    Add {
        /// The LP token contract address
        lp_token: String,
        /// The slice of ASTRO emissions this generator gets
        alloc_point: Uint64,
        /// This flag determines whether the pool gets 3rd party token rewards
        has_asset_rewards: bool,
        /// The address of the 3rd party reward proxy contract
        reward_proxy: Option<String>,
    },
    /// ## Description
    /// Update the given pool's ASTRO allocation slice
    /// ## Executor
    /// Only the owner can execute this.
    Set {
        /// The address of the LP token contract address whose allocation we change
        lp_token: String,
        /// The new allocation
        alloc_point: Uint64,
        /// This flag determines whether the pool gets 3rd party token rewards
        has_asset_rewards: bool,
    },
    /// ## Description
    /// Updates reward variables for multiple pools
    MassUpdatePools {},
    /// ## Description
    /// Updates reward variables for a specific pool
    UpdatePool {
        /// the LP token contract address
        lp_token: String,
    },
    /// ## Description
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
        /// The amount to withdraw
        amount: Uint128,
    },
    /// ## Description
    /// Withdraw LP tokens from the Generator without withdrawing outstanding rewards
    EmergencyWithdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
    },
    /// ## Description
    /// Allowed reward proxy contracts that can interact with the Generator
    SetAllowedRewardProxies {
        /// The full list of allowed proxy contracts
        proxies: Vec<String>,
    },
    /// ## Description
    /// Sends orphan proxy rewards (which were left behind after emergency withdrawals) to another address
    SendOrphanProxyReward {
        /// The transfer recipient
        recipient: String,
        /// The address of the LP token contract for which we send orphaned rewards
        lp_token: String,
    },
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ## Description
    /// Set a new amount of ASTRO to distribute per block
    /// ## Executor
    /// Only the owner can execute this.
    SetTokensPerBlock {
        /// The new amount of ASTRO to distro per block
        amount: Uint128,
    },
    /// ## Description
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this.
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the contract owner
        expires_in: u64,
    },
    /// ## Description
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// ## Description
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// PoolLength returns the length of the array that contains all the instantiated pool generators
    PoolLength {},
    /// Deposit returns the LP token amount deposited in a specific generator
    Deposit { lp_token: String, user: String },
    /// PendingToken returns the amount of rewards that can be claimed by an account that deposited a specific LP token in a generator
    PendingToken { lp_token: String, user: String },
    /// Config returns the main contract parameters
    Config {},
    /// RewardInfo returns reward information for a specified LP token
    RewardInfo { lp_token: String },
    /// OrphanProxyRewards returns orphaned reward information for the specified LP token
    OrphanProxyRewards { lp_token: String },
    /// PoolInfo returns information about a pool associated with the specified LP token alongside
    /// the total pending amount of ASTRO and proxy rewards claimable by generator stakers (for that LP token)
    PoolInfo { lp_token: String },
    /// SimulateFutureReward returns the amount of ASTRO that will be distributed until a future block and for a specific generator
    SimulateFutureReward { lp_token: String, future_block: u64 },
}

/// ## Description
/// This structure holds the response returned when querying the total length of the array that keeps track of instantiated generators
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolLengthResponse {
    pub length: usize,
}

/// ## Description
/// This structure holds the response returned when querying the amount of pending rewards that can be withdrawn from a 3rd party
/// rewards contract
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingTokenResponse {
    /// The amount of pending ASTRO
    pub pending: Uint128,
    /// The amount of pending 3rd party reward tokens
    pub pending_on_proxy: Option<Uint128>,
}

/// ## Description
/// This structure holds the response returned when querying for the token addresses used to reward a specific generator
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    /// The address of the base reward token
    pub base_reward_token: Addr,
    /// The address of the 3rd party reward token
    pub proxy_reward_token: Option<Addr>,
}

/// ## Description
/// This structure holds the response returned when querying for a pool's information
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoResponse {
    /// The slice of ASTRO that this pool's generator gets per block
    pub alloc_point: Uint64,
    /// Amount of ASTRO tokens being distributed per block to this LP pool
    pub astro_tokens_per_block: Uint128,
    /// The last block when token emissions were snapshotted (distributed)
    pub last_reward_block: u64,
    /// Current block number. Useful for computing APRs off-chain
    pub current_block: u64,
    /// Total amount of ASTRO rewards already accumulated per LP token staked
    pub accumulated_rewards_per_share: Decimal,
    /// Pending amount of total ASTRO rewards which are claimable by stakers right now
    pub pending_astro_rewards: Uint128,
    /// The address of the 3rd party reward proxy contract
    pub reward_proxy: Option<Addr>,
    /// Pending amount of total proxy rewards which are claimable by stakers right now
    pub pending_proxy_rewards: Option<Uint128>,
    /// Total amount of 3rd party token rewards already accumulated per LP token staked
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// Reward balance for the dual rewards proxy before updating accrued rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// The amount of orphan proxy rewards which are left behind by emergency withdrawals and not yet transferred out
    pub orphan_proxy_rewards: Uint128,
    /// Total amount of lp tokens staked in the pool's generator
    pub lp_supply: Uint128,
}

/// ## Description
/// This structure holds the response returned when querying the contract for general parameters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// ASTRO token contract address
    pub astro_token: Addr,
    /// Total amount of ASTRO distributed per block
    pub tokens_per_block: Uint128,
    /// Sum of total allocation points across all active generators
    pub total_alloc_point: Uint64,
    /// Start block for ASTRO incentives
    pub start_block: Uint64,
    /// List of 3rd party reward proxies allowed to interact with the Generator contract
    pub allowed_reward_proxies: Vec<Addr>,
    /// The ASTRO vesting contract address
    pub vesting_contract: Addr,
}

/// ## Description
/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// This structure describes custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Deposit performs a token deposit on behalf of the message sender.
    Deposit {},
    /// DepositFor performs a token deposit on behalf of another address that's not the message sender.
    DepositFor(Addr),
}
