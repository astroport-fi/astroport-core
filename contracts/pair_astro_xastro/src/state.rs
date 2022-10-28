use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

/// This structure stores a ASTRO-xASTRO pool's params.
#[cw_serde]
pub struct Params {
    /// ASTRO token contract address.
    pub astro_addr: Addr,
    /// xASTRO token contract address.
    pub xastro_addr: Addr,
    /// Astroport Staking contract address.
    pub staking_addr: Addr,
}
