use cosmwasm_std::{Addr, Decimal, Uint64};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The structure the Maker configuration for version 1.0.0.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    /// Address that's allowed to set contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The xASTRO staking contract address
    pub staking_contract: Addr,
    /// The vxASTRO fee distributor contract address
    pub governance_contract: Option<Addr>,
    /// The percentage of fees that go to the vxASTRO fee distributor
    pub governance_percent: Uint64,
    /// The ASTRO token address
    pub astro_token_contract: Addr,
    /// The max spread allowed when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
}

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");
