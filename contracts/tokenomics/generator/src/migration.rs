use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure stores the parameters for a generator (in the upgraded version of the Generator contract).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoV100 {
    /// This is the share of ASTRO rewards that this generator receives every block
    pub alloc_point: Uint64,
    /// Accumulated amount of rewards per LP unit. Used for reward calculations
    pub last_reward_block: Uint64,
    /// This is the accrued amount of rewards up to the latest checkpoint
    pub accumulated_rewards_per_share: Decimal,
    /// The 3rd party proxy reward contract
    pub reward_proxy: Option<Addr>,
    /// This is the accrued amount of 3rd party rewards up to the latest checkpoint
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// This is the balance of 3rd party proxy rewards that the proxy had before a reward snapshot
    pub proxy_reward_balance_before_update: Uint128,
    /// The orphaned proxy rewards which are left behind by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
}

pub const POOL_INFOV100: Map<&Addr, PoolInfoV100> = Map::new("pool_info");
