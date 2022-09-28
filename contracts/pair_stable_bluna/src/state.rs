use astroport::asset::PairInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256, Uint128};
use cw_storage_plus::{Item, Map};

/// ## Description
/// This structure stores the main stableswap pair parameters.
#[cw_serde]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract update the asset cumulative prices
    pub block_time_last: u64,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    // This is the current amplification used in the pool
    pub init_amp: u64,
    // This is the start time when amplification starts to scale up or down
    pub init_amp_time: u64,
    // This is the target amplification to reach at `next_amp_time`
    pub next_amp: u64,
    // This is the timestamp when the current pool amplification should be `next_amp`
    pub next_amp_time: u64,
    /// Contract to claim bLUNA rewards from
    pub bluna_rewarder: Addr,
    /// The generator address used for determining users' bLUNA reward shares (while they are staked)
    pub generator: Addr,
}

/// Stores the pool configuration
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the address of the contract that holds bLUNA rewards
pub const BLUNA_REWARD_HOLDER: Item<Addr> = Item::new("bluna_reward_holder");

/// Stores the bLUNA global reward index
pub const BLUNA_REWARD_GLOBAL_INDEX: Item<Decimal256> = Item::new("bluna_reward_global_index");

/// Stores each LP's reward index to know when is the last time when they claimed
pub const BLUNA_REWARD_USER_INDEXES: Map<&Addr, Decimal256> = Map::new("bluna_reward_user_indexes");
