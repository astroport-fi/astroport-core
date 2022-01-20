use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of maker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
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
    /// the flag which defines whether rewards collecting is enabled or not
    pub rewards_enabled: bool,
    /// the number of blocks when rewards should be distributed evenly
    pub pre_upgrade_blocks: u64,
    /// the last block when pre-upgrade ASTRO fee was distributed
    pub last_distribution_block: u64,
    /// the remainder of pre-upgrade ASTRO fee
    pub remainder_reward: Uint128,
    /// the amount of collected ASTRO fee before enabling rewards distribution
    pub pre_upgrade_astro_amount: Uint128,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// ## Description
/// Stores bridges to swap fees while collecting
pub const BRIDGES: Map<String, AssetInfo> = Map::new("bridges");
