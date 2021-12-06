use astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr, Uint64};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub factory_contract: Addr,
    pub staking_contract: Addr,
    pub governance_contract: Option<Addr>,
    pub governance_percent: Uint64,
    pub astro_token_contract: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
