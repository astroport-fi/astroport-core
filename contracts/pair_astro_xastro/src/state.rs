use cosmwasm_std::Addr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores a ASTRO-xASTRO pool's params.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Params {
    /// ASTRO token contract address.
    pub astro_addr: Addr,
    /// xASTRO token contract address.
    pub xastro_addr: Addr,
    /// Astroport Staking contract address.
    pub staking_addr: Addr,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
