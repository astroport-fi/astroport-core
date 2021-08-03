use cosmwasm_std::{Addr, Decimal, Timestamp, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::ExecuteData;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ProposalState {
    Pending,
    Active,
    Canceled,
    Defeated,
    Succeeded,
    Queued,
    Expired,
    Executed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub token: Addr,
    pub guardian: Addr,
    pub admin: Addr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub voting_delay_period: u64,
    pub timelock_period: u64,
    pub proposal_weight: Uint128,
    pub expiration_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        link: Option<String>,
        execute_data: Option<Vec<ExecuteData>>,
    },
    Vote {
        proposal_id: u64,
        support: bool,
    },

    Queue {
        proposal_id: u64,
    },
    Execute {
        proposal_id: u64,
    },
    Cancel {
        proposal_id: u64,
    },

    SetDelay {
        delay: u64,
    },
    AcceptAdmin {},
    SetPendingAdmin {
        admin: Addr,
    },
    UpdateGovernanceConfig {
        guardian: Option<Addr>,
        timelock_period: Option<u64>,
        expiration_period: Option<u64>,
        quorum: Option<Decimal>,
        voting_period: Option<u64>,
        voting_delay_period: Option<u64>,
        threshold: Option<Decimal>,
        proposal_weight: Option<Uint128>,
    },
    CreateLock {
        amount: Uint128,
        lock: Timestamp,
    },
    IncreaseAmount {
        amount: Uint128,
    },
    IncreaseUnlockTime {
        unlock_time: Timestamp,
    },
    Deposit {
        user: Addr,
        amount: Uint128,
    },
    Withdraw {},
    Checkpoint {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState { proposal_id: u64 },
    GetProposal { proposal_id: u64 },
    GetBalanceOf { user: Addr },
    GetBalanceOfAt { user: Addr, block: u64 },
    GetLockedBalance { user: Addr },
    GetTotalSupply {},
    GetTotalSupplyAt { block: u64 },
    LockedEnd { user: Addr },
    Config {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub state: ProposalState,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TimeResponse {
    pub time: Timestamp,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub guardian: Addr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub timelock_period: u64,
    pub expiration_period: u64,
    pub proposal_weight: Uint128,
    pub voting_delay_period: u64,
}
