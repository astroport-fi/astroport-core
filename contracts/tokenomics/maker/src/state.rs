use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw_storage_plus::{Item, Map};

/// ## Description
/// This structure stores the main paramters for the Maker contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to set contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The xASTRO staking contract address.
    pub staking_contract: Option<Addr>,
    /// Default bridge asset (Terra1 - LUNC, Terra2 - LUNA, etc.)
    pub default_bridge: Option<AssetInfo>,
    /// The vxASTRO fee distributor contract address
    pub governance_contract: Option<Addr>,
    /// The percentage of fees that go to the vxASTRO fee distributor
    pub governance_percent: Uint64,
    /// The ASTRO token asset info
    pub astro_token: AssetInfo,
    /// The max spread allowed when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
    /// The flag which determines whether accrued ASTRO from fee swaps is being distributed or not
    pub rewards_enabled: bool,
    /// The number of blocks over which ASTRO that accrued pre-upgrade will be distributed
    pub pre_upgrade_blocks: u64,
    /// The last block until which pre-upgrade ASTRO will be distributed
    pub last_distribution_block: u64,
    /// The remainder of pre-upgrade ASTRO to distribute
    pub remainder_reward: Uint128,
    /// The amount of collected ASTRO before enabling rewards distribution
    pub pre_upgrade_astro_amount: Uint128,
}

/// ## Description
/// Stores the contract configuration at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// ## Description
/// Stores bridge tokens used to swap fee tokens to ASTRO
pub const BRIDGES: Map<String, AssetInfo> = Map::new("bridges");
