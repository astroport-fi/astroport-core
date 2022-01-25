use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main information of pool
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoV100 {
    /// Allocation point is used to control reward distribution among the pools
    pub alloc_point: Uint64,
    /// Accumulated amount of reward per share unit. Used for reward calculations
    pub last_reward_block: Uint64,
    pub accumulated_rewards_per_share: Decimal,
    /// the reward proxy contract
    pub reward_proxy: Option<Addr>,
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// the orphan proxy rewards which are left by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
}

pub const POOL_INFOV100: Map<&Addr, PoolInfoV100> = Map::new("pool_info");
