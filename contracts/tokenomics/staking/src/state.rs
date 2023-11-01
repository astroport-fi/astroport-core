use cosmwasm_schema::cw_serde;
use cw_storage_plus::Item;

/// This structure stores the main parameters for the staking contract.
#[cw_serde]
pub struct Config {
    /// The ASTRO token denom
    pub astro_denom: String,
    /// The xASTRO token denom
    pub xastro_denom: String,
    // TODO: Do we want this?
    pub tracking_code_id: u64,
    // TODO: Make addr?
    pub tracking_contract_address: String,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
