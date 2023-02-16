use astroport_pair_bonded::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api};

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

/// This structure stores a ASTRO-xASTRO pool's init params.
#[cw_serde]
pub struct InitParams {
    /// ASTRO token contract address.
    pub astro_addr: String,
    /// xASTRO token contract address.
    pub xastro_addr: String,
    /// Astroport Staking contract address.
    pub staking_addr: String,
}

impl InitParams {
    pub fn try_into_params(self, api: &dyn Api) -> Result<Params, ContractError> {
        Ok(Params {
            astro_addr: api.addr_validate(&self.astro_addr)?,
            xastro_addr: api.addr_validate(&self.xastro_addr)?,
            staking_addr: api.addr_validate(&self.staking_addr)?,
        })
    }
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}
