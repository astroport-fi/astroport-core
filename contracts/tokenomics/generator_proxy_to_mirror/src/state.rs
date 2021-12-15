use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// ## Description
/// This structure describes the main controls configs of generator_proxy_to_mirror contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the generator contract address
    pub generator_contract_addr: Addr,
    /// the pair contract address
    pub pair_addr: Addr,
    /// the contract address for liquidity pool token
    pub lp_token_addr: Addr,
    /// the reward contract address
    pub reward_contract_addr: Addr,
    /// the reward token contract address
    pub reward_token_addr: Addr,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
