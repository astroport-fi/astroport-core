use crate::asset::Asset;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{from_slice, Addr, CosmosMsg, Decimal, Empty, StdResult, Uint128};
use cw3::Vote;
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use cw_utils::{Duration, Expiration, Threshold, ThresholdResponse};
use std::fmt::{Display, Formatter};

pub const TOTAL_WEIGHT: u64 = 2;
pub const DEFAULT_WEIGHT: u64 = 1;

#[cw_serde]
pub struct Config {
    pub threshold: Threshold,
    pub total_weight: u64,
    pub max_voting_period: Duration,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The generator contract address
    pub generator_addr: Addr,
    /// Address allowed to change contract parameters
    pub manager1: Addr,
    /// Address allowed to change contract parameters
    pub manager2: Addr,
    /// The target pool is the one where the contract can LP NTRN and ASTRO at the current pool price
    pub target_pool: Option<Addr>,
    /// This is the pool into which liquidity will be migrated from the target pool.
    pub migration_pool: Option<Addr>,
    /// Allows to withdraw funds for both managers
    pub rage_quit_started: bool,
    /// This is the denom that the first manager will manage.
    pub denom1: String,
    /// This is the denom that the second manager will manage.
    pub denom2: String,
}

#[cw_serde]
pub struct ConfigResponse {
    pub threshold: ThresholdResponse,
    pub max_voting_period: Duration,
    pub manager1: String,
    pub manager2: String,
    pub target_pool: Option<Addr>,
    pub migration_pool: Option<Addr>,
    pub rage_quit_started: bool,
    pub denom1: String,
    pub denom2: String,
    pub factory: String,
    pub generator: String,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub factory_addr: String,
    pub generator_addr: String,
    pub max_voting_period: Duration,
    /// Address allowed to change contract parameters
    pub manager1: String,
    /// Address allowed to change contract parameters
    pub manager2: String,
    /// This is the denom that the first manager will manage.
    pub denom1: String,
    /// This is the denom that the second manager will manage.
    pub denom2: String,
    /// The target pool is the one where the contract can LP NTRN and ASTRO at the current pool price
    pub target_pool: Option<String>,
}

#[cw_serde]
#[derive(Hash, Eq)]
pub enum PoolType {
    Target,
    Migration,
}

impl Display for PoolType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolType::Target => f.write_str("target"),
            PoolType::Migration => f.write_str("migration"),
        }
    }
}

#[cw_serde]
#[derive(Hash, Eq)]
pub enum MultisigRole {
    Manager1,
    Manager2,
}

impl Display for MultisigRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MultisigRole::Manager1 => f.write_str("manager1"),
            MultisigRole::Manager2 => f.write_str("manager2"),
        }
    }
}

impl MultisigRole {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            MultisigRole::Manager1 => "manager1".as_bytes(),
            MultisigRole::Manager2 => "manager2".as_bytes(),
        }
    }
}

impl<'a> PrimaryKey<'a> for &MultisigRole {
    type Prefix = ();

    type SubPrefix = ();

    type Suffix = Self;

    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl<'a> Prefixer<'a> for &MultisigRole {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl KeyDeserialize for &MultisigRole {
    type Output = MultisigRole;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        from_slice(&value)
    }
}

#[cw_serde]
pub struct ProvideParams {
    /// The slippage tolerance that allows liquidity provision only if the price in the pool
    /// doesn't move too much
    pub slippage_tolerance: Option<Decimal>,
    /// Determines whether the LP tokens minted for the user is auto_staked in the Generator contract
    pub auto_stake: Option<bool>,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateConfig {
        generator: Option<String>,
        factory: Option<String>,
    },
    SetupPools {
        /// The target pool is the one where the contract can LP NTRN and ASTRO at the current pool price
        target_pool: Option<String>,
        /// This is the pool into which liquidity will be migrated from the target pool.
        migration_pool: Option<String>,
    },
    SetupMaxVotingPeriod {
        max_voting_period: Duration,
    },
    /// Withdraws coins from the target pool. Also it is possible to withdraw LP from the target
    /// pool and make provide LP to the migration pool in the same transaction.
    WithdrawTargetPoolLP {
        withdraw_amount: Option<Uint128>,
        provide_params: Option<ProvideParams>,
    },
    /// Withdraws coins from the specified pool.
    WithdrawRageQuitLP {
        pool_type: PoolType,
        withdraw_amount: Option<Uint128>,
    },
    /// Withdraw LP tokens from the Generator
    WithdrawGenerator {
        /// The amount to withdraw
        amount: Option<Uint128>,
    },
    /// Update generator rewards and returns them to the Multisig.
    ClaimGeneratorRewards {},
    /// Deposit the target LP tokens to the generator
    DepositGenerator {
        /// The amount to deposit
        amount: Option<Uint128>,
    },
    /// Provides liquidity to the specified pool
    ProvideLiquidity {
        pool_type: PoolType,
        /// The assets available in the pool
        assets: Vec<Asset>,
        /// The slippage tolerance that allows liquidity provision only if the price in the pool doesn't move too much
        slippage_tolerance: Option<Decimal>,
        /// Determines whether the LP tokens minted for the user is auto_staked in the Generator contract
        auto_stake: Option<bool>,
        /// The receiver of LP tokens
        receiver: Option<String>,
    },
    /// Transfers manager coins and other coins from the shared_multisig.
    /// Executor: manager1 or manager2.
    Transfer {
        asset: Asset,
        recipient: Option<String>,
    },
    CompleteTargetPoolMigration {},
    StartRageQuit {},
    Propose {
        title: String,
        description: String,
        msgs: Vec<CosmosMsg<Empty>>,
        // note: we ignore API-spec'd earliest if passed, always opens immediately
        latest: Option<Expiration>,
    },
    Vote {
        proposal_id: u64,
        vote: Vote,
    },
    Execute {
        proposal_id: u64,
    },
    Close {
        proposal_id: u64,
    },
    /// Creates a proposal to change contract manager1. The validity period for the proposal is set
    /// in the `expires_in` variable.
    ProposeNewManager1 {
        /// Newly proposed contract manager
        new_manager: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// Removes the existing offer to change contract manager1.
    DropManager1Proposal {},
    /// Used to claim a new contract manager1.
    ClaimManager1 {},
    /// Creates a proposal to change contract manager2. The validity period for the proposal is set
    /// in the `expires_in` variable.
    ProposeNewManager2 {
        /// Newly proposed contract
        new_manager: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// Removes the existing offer to change contract manager2.
    DropManager2Proposal {},
    /// Used to claim a new second manager of contract.
    ClaimManager2 {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(cw3::ProposalResponse)]
    Proposal { proposal_id: u64 },
    #[returns(cw3::ProposalListResponse)]
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(cw3::ProposalListResponse)]
    ReverseProposals {
        start_before: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(cw3::VoteResponse)]
    Vote { proposal_id: u64, voter: String },
    #[returns(cw3::VoteListResponse)]
    ListVotes { proposal_id: u64 },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multisig_role() {
        assert_eq!(MultisigRole::Manager1.as_bytes(), "manager1".as_bytes());
        assert_eq!(MultisigRole::Manager2.as_bytes(), "manager2".as_bytes());
    }

    #[test]
    fn test_multisig_role_display() {
        assert_eq!(MultisigRole::Manager1.to_string(), "manager1");
        assert_eq!(MultisigRole::Manager2.to_string(), "manager2");
    }
}
