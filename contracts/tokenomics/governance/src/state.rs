use std::cmp::Ordering;

use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// static KEY_CONFIG: &[u8] = b"config";
// static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct ExecuteData {
    pub order: u64,
    pub contract: Addr,
    pub msg: Binary,
}

impl Eq for ExecuteData {}

impl Ord for ExecuteData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}

impl PartialOrd for ExecuteData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ExecuteData {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Proposal {
    // Unique id for looking up a proposal
    pub id: u64,
    // Creator of the proposal
    pub proposer: Addr,
    // The timestamp that the proposal will be available for execution, set once the vote succeeds
    pub eta: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_data: Option<Vec<ExecuteData>>,
    // The block at which voting begins: holders must delegate their votes prior to this block
    pub start_block: u64,
    // The block at which voting ends: votes must be cast prior to this block
    pub end_block: u64,
    // Current number of votes in favor of this proposal
    pub for_votes: Uint128,
    // Current number of votes in opposition to this proposal
    pub against_votes: Uint128,
    // Flag marking whether the proposal has been canceled
    pub canceled: bool,
    // Flag marking whether the proposal has been executed
    pub executed: bool,
    // Receipts of ballots for the entire set of voters
    //pub receipts: Map<&Addr, Receipt>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, JsonSchema)]
pub struct LockedBalance {
    pub amount: Uint128,
    pub end: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, JsonSchema, Copy)]
pub struct Point {
    pub bias: Uint128,
    pub slope: Uint128,
    pub ts: u64,
    pub blk: u64,
}

// Ballot receipt record for a voter
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Receipt {
    // Whether or not a vote has been cast
    pub has_voted: bool,
    // Whether or not the voter supports the proposal
    pub support: bool,
    // The number of votes the voter had, which were cast
    pub votes: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub owner: Addr,
    // The total number of proposals
    pub proposal_count: u64,
    // The address of the Governor Guardian
    pub supply: Uint128,
    pub epoch: usize,
    pub point_history: Vec<Point>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub guardian: Addr,
    //Timelock variable
    pub admin: Addr,
    pub pending_admin: Addr,
    pub timelock_period: u64,
    pub expiration_period: u64,
    pub quorum: Decimal,
    pub voting_period: u64,
    pub voting_delay_period: u64,
    pub threshold: Decimal,
    pub proposal_weight: Uint128,
    pub xtrs_token: Addr,
}

// pub fn store_config(storage: &mut dyn Storage) -> Singleton<Config> {
//     singleton(storage, KEY_CONFIG)
// }
//
// pub fn read_config(storage: &dyn Storage) -> ReadonlySingleton<Config> {
//     singleton_read(storage, KEY_CONFIG)
// }

// pub fn store_state(storage: &mut dyn Storage) -> Singleton<State> {
//     singleton(storage, KEY_STATE)
// }
//
// pub fn read_state(storage: &dyn Storage) -> ReadonlySingleton<State> {
//     singleton_read(storage, KEY_STATE)
// }

pub const CONFIG: Item<Config> = Item::new("config");
pub const GOVERNANCE_SATE: Item<State> = Item::new("state");

pub const QUEUED_TRANSACTIONS: Map<u64, bool> = Map::new("queuedTransactions");
pub const LOCKED: Map<&Addr, LockedBalance> = Map::new("lockedBalances");
pub const USER_POINT_HISTORY: Map<&Addr, Vec<Point>> = Map::new("user_point_history");
//user -> Point[user_epoch]
pub const USER_POINT_EPOCH: Map<&Addr, usize> = Map::new("user_point_epoch");
pub const SLOPE_CHANGES: Map<U64Key, Uint128> = Map::new("slope_changes");
// time -> signed slope change
// Receipts of ballots for the entire set of voters
pub const RECEIPTS: Map<(U64Key, &Addr), Receipt> = Map::new("receipts");
// The official record of all proposals ever proposed
pub const PROPOSALS: Map<U64Key, Proposal> = Map::new("proposals");
// The latest proposal for each proposer
pub const LATEST_PROPOSAL_IDS: Map<&Addr, u64> = Map::new("latest_proposal_ids");
