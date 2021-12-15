use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of staking contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the ASTRO token contract address
    pub astro_token_addr: Addr,
    /// the xASTRO token contract address
    pub xastro_token_addr: Addr,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
