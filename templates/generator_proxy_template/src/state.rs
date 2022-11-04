use cosmwasm_schema::cw_serde;

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// This structure holds the main parameters of the generator_proxy_template contract.
#[cw_serde]
pub struct Config {
    /// The Generator contract address
    pub generator_contract_addr: Addr,
    /// The target Astroport pair contract address
    pub pair_addr: Addr,
    /// The contract address for the Astroport LP token associated with pair_addr
    pub lp_token_addr: Addr,
    /// The 3rd party reward contract address
    pub reward_contract_addr: Addr,
    /// The 3rd party reward token
    pub reward_token_addr: Addr,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
