use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// ## Description
/// This structure stores the main params for the generator_proxy_template contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The Generator contract address
    pub generator_contract_addr: Addr,
    /// The target Astroprot pair contract address
    pub pair_addr: Addr,
    /// The contract address for the Astroport LP token associated with pair_addr
    pub lp_token_addr: Addr,
    /// The reward contract address (3rd party staking contract)
    pub reward_contract_addr: Addr,
    /// The 3rd party reward token
    pub reward_token_addr: Addr,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
