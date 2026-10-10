use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256, Uint128};
use cw_storage_plus::{Item, Map};

use astroport::common::OwnershipProposal;

#[cw_serde]
pub struct Config {
    /// Can change the configuration
    pub owner: Addr,
    /// The denom staked for yASTRO, 1:1
    pub astro_denom: String,
    /// Seconds between unstaking and being able to withdraw
    pub unbonding_period: u64,
    /// Astroport Incentives, where registered pools' rewards are forwarded for their LP stakers
    pub incentives: Option<Addr>,
}

/// ASTRO on its way out: burned yASTRO that can be withdrawn after `release_at`
#[cw_serde]
pub struct Unbonding {
    pub amount: Uint128,
    pub release_at: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Reward denoms `deposit_rewards` accepts
pub const REWARD_DENOMS: Item<Vec<String>> = Item::new("reward_denoms");
/// Reward per yASTRO paid out so far, for every denom ever deposited
pub const GLOBAL_INDEX: Map<&str, Decimal256> = Map::new("global_index");
/// Deposits made while nobody held yASTRO, added to the next deposit of the same denom
pub const UNDISTRIBUTED: Map<&str, Uint128> = Map::new("undistributed");
/// The global index an account's rewards were last settled at
pub const USER_INDEX: Map<(&Addr, &str), Decimal256> = Map::new("user_index");
/// Settled, unclaimed rewards
pub const PENDING: Map<(&Addr, &str), Uint128> = Map::new("pending");

pub const UNBONDINGS: Map<&Addr, Vec<Unbonding>> = Map::new("unbondings");
pub const TOTAL_UNBONDING: Item<Uint128> = Item::new("total_unbonding");

/// Pools holding yASTRO whose rewards go to their LP stakers in Incentives: pool -> LP token
pub const POOLS: Map<&Addr, String> = Map::new("pools");
