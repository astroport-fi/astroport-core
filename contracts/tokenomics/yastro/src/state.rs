use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin, Decimal256, Uint128};
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
    /// Astroport Incentives, where pools' rewards are forwarded for their LP stakers
    pub incentives: Option<Addr>,
}

/// ASTRO on its way out: burned yASTRO that can be withdrawn after `release_at`
#[cw_serde]
pub struct Unbonding {
    pub amount: Uint128,
    pub release_at: u64,
}

/// How one reward denom is paid out. Deposits made during an epoch are queued and streamed
/// evenly to yASTRO holders over the next epoch (Astroport's weeks, starting Monday 00:00 UTC).
#[cw_serde]
pub struct Stream {
    /// Reward per yASTRO paid out so far
    pub index: Decimal256,
    /// Reward paid out per second during the current epoch
    pub rate: Decimal256,
    /// Paid out over the next epoch: this epoch's deposits, plus anything streamed while nobody
    /// held yASTRO
    pub queued: Decimal256,
    /// When the current epoch ends
    pub epoch_end: u64,
    /// When `index` was last brought up to date
    pub last_update: u64,
    /// Everything ever deposited
    pub total_deposited: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Reward denoms `deposit_rewards` accepts
pub const REWARD_DENOMS: Item<Vec<String>> = Item::new("reward_denoms");
/// Every denom ever deposited
pub const STREAMS: Map<&str, Stream> = Map::new("streams");
/// The stream index an account's rewards were last settled at
pub const USER_INDEX: Map<(&Addr, &str), Decimal256> = Map::new("user_index");
/// Settled, unclaimed rewards. Fractions stay here until they add up to a whole unit.
pub const PENDING: Map<(&Addr, &str), Decimal256> = Map::new("pending");

pub const UNBONDINGS: Map<&Addr, Vec<Unbonding>> = Map::new("unbondings");
pub const TOTAL_UNBONDING: Item<Uint128> = Item::new("total_unbonding");

/// Pool rewards sent to Incentives, by reply id, so a failed schedule can go back to the pool
pub const FORWARDS: Map<u64, (Addr, Coin)> = Map::new("forwards");
pub const NEXT_FORWARD_ID: Item<u64> = Item::new("next_forward_id");
