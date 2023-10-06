use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// This structure stores the main parameters for the staking contract.
#[cw_serde]
pub struct Config {
    /// The ASTRO token denom
    pub astro_denom: String,
    /// The xASTRO token denom
    pub xastro_denom: String,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
