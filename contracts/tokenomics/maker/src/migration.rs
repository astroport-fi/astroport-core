use cosmwasm_std::{Addr, Decimal, Uint64};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The structure describes main maker config for version 1.0.0.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    /// contract address that used for controls settings
    pub owner: Addr,
    /// the factory contract address
    pub factory_contract: Addr,
    /// the staking contract address
    pub staking_contract: Addr,
    /// the governance contract address
    pub governance_contract: Option<Addr>,
    /// the governance percent
    pub governance_percent: Uint64,
    /// the ASTRO token address
    pub astro_token_contract: Addr,
    /// the max spread
    pub max_spread: Decimal,
}

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");
