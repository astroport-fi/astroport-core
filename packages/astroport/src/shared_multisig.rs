use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{from_slice, Addr, CosmosMsg, Empty, StdResult};
use cw3::Vote;
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use cw_utils::{Duration, Expiration, Threshold, ThresholdResponse};
use std::fmt::{Display, Formatter};

#[cw_serde]
pub struct Config {
    pub threshold: Threshold,
    pub total_weight: u64,
    pub max_voting_period: Duration,
    /// Address allowed to change contract parameters
    pub dao: Addr,
    /// Address allowed to change contract parameters
    pub manager: Addr,
}

#[cw_serde]
pub struct ConfigResponse {
    pub threshold: ThresholdResponse,
    pub max_voting_period: Duration,
    pub dao: Addr,
    pub manager: Addr,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub max_voting_period: Duration,
    /// Address allowed to change contract parameters
    pub dao: String,
    /// Address allowed to change contract parameters
    pub manager: String,
}

#[cw_serde]
#[derive(Hash, Eq)]
pub enum MultisigRole {
    Manager,
    Dao,
}

impl Display for MultisigRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MultisigRole::Manager => f.write_str("manager"),
            MultisigRole::Dao => f.write_str("dao"),
        }
    }
}

impl MultisigRole {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            MultisigRole::Manager => "manager".as_bytes(),
            MultisigRole::Dao => "dao".as_bytes(),
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
pub enum ExecuteMsg {
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
    UpdateConfig {
        max_voting_period: Duration,
    },
    Close {
        proposal_id: u64,
    },
    /// Creates a proposal to change contract manager. The validity period for the proposal is set
    /// in the `expires_in` variable.
    ProposeNewManager {
        /// Newly proposed contract manager
        manager: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// Removes the existing offer to change contract manager.
    DropManagerProposal {},
    /// Used to claim a new contract manager.
    ClaimManager {},
    /// Creates a proposal to change contract DAO. The validity period for the proposal is set
    /// in the `expires_in` variable.
    ProposeNewDao {
        /// Newly proposed contract DAO
        dao: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// Removes the existing offer to change contract manager.
    DropDaoProposal {},
    /// Used to claim a new contract DAO.
    ClaimDao {},
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
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}
