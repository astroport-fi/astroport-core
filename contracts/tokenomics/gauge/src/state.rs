use cosmwasm_std::{Addr, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static KEY_CONFIG: &[u8] = b"config";

// Info of each user.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub amount: Uint128,
    pub reward_debt: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub lp_token: Addr,
    pub alloc_point: u64,
    pub last_reward_block: u64,
    pub acc_per_share: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    // The xTRS TOKEN!
    pub xtrs_token: Addr,
    // xTRS balance
    pub xtrs_token_balance: Uint128,
    // Dev address.
    pub dev_addr: Addr,
    // Block number when bonus xTRS period ends.
    pub bonus_end_block: u64,
    // xTRS tokens created per block.
    pub tokens_per_block: Uint128,
    // Info of each pool.
    pub pool_info: Vec<PoolInfo>,
    // Total allocation poitns. Must be the sum of all allocation points in all pools.
    pub total_alloc_point: u64,
    // The block number when xTRS mining starts.
    pub start_block: u64,
}

pub fn store_config(storage: &mut dyn Storage) -> Singleton<Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config(storage: &dyn Storage) -> ReadonlySingleton<Config> {
    singleton_read(storage, KEY_CONFIG)
}

pub const USER_INFO: Map<&Addr, Vec<UserInfo>> = Map::new("user_info");
pub const LP_TOKEN_BALANCES: Map<(&Addr, &Addr), Uint128> = Map::new("lp_token_balance");
