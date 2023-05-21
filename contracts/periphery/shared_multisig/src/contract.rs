use std::cmp::Ordering;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, BlockInfo, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Order,
    Response, StdError, StdResult,
};

use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::shared_multisig::{
    Config, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, MultisigRole, QueryMsg,
};
use cw2::set_contract_version;
use cw3::{
    Proposal, ProposalListResponse, ProposalResponse, Status, Vote, VoteInfo, VoteListResponse,
    VoteResponse, Votes,
};
use cw_storage_plus::Bound;
use cw_utils::{Duration, Expiration, Threshold};

use crate::error::ContractError;
use crate::state::{
    next_id, BALLOTS, CONFIG, DAO_PROPOSAL, DEFAULT_LIMIT, MANAGER_PROPOSAL, MAX_LIMIT, PROPOSALS,
};

// version info for migration info
const CONTRACT_NAME: &str = "astroport-shared-multisig";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const TOTAL_WEIGHT: u64 = 2;
const DEFAULT_WEIGHT: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = Config {
        threshold: Threshold::AbsoluteCount {
            weight: TOTAL_WEIGHT,
        },
        total_weight: TOTAL_WEIGHT,
        max_voting_period: msg.max_voting_period,
        dao: deps.api.addr_validate(&msg.dao)?,
        manager: deps.api.addr_validate(&msg.manager)?,
    };
    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            msgs,
            latest,
        } => execute_propose(deps, env, info, title, description, msgs, latest),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::UpdateConfig { max_voting_period } => {
            update_config(deps, env, info, max_voting_period)
        }
        ExecuteMsg::ProposeNewManager {
            manager,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;
            propose_new_owner(
                deps,
                info,
                env,
                manager,
                expires_in,
                config.manager,
                MANAGER_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropManagerProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.manager, MANAGER_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimManager {} => {
            claim_ownership(deps, info, env, MANAGER_PROPOSAL, |deps, new_manager| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.manager = new_manager;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::ProposeNewDao { dao, expires_in } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(deps, info, env, dao, expires_in, config.dao, DAO_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::DropDaoProposal {} => {
            let config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.dao, DAO_PROPOSAL).map_err(Into::into)
        }
        ExecuteMsg::ClaimDao {} => {
            claim_ownership(deps, info, env, DAO_PROPOSAL, |deps, new_dao| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.dao = new_dao;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    msgs: Vec<CosmosMsg>,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<Response<Empty>, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.dao && info.sender != cfg.manager {
        return Err(ContractError::Unauthorized {});
    }

    // max expires also used as default
    let max_expires = cfg.max_voting_period.after(&env.block);
    let mut expires = latest.unwrap_or(max_expires);
    let comp = expires.partial_cmp(&max_expires);
    if let Some(Ordering::Greater) = comp {
        expires = max_expires;
    } else if comp.is_none() {
        return Err(ContractError::WrongExpiration {});
    }

    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires,
        msgs,
        status: Status::Open,
        votes: Votes::yes(DEFAULT_WEIGHT),
        threshold: cfg.threshold,
        total_weight: cfg.total_weight,
        proposer: info.sender.clone(),
        deposit: None,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id, &prop)?;

    // add the first yes vote from voter
    if info.sender == cfg.dao {
        BALLOTS.save(deps.storage, (id, &MultisigRole::Dao), &Vote::Yes)?;
    } else {
        BALLOTS.save(deps.storage, (id, &MultisigRole::Manager), &Vote::Yes)?;
    }

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response<Empty>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.dao && info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // Allow voting on Passed and Rejected proposals too
    if ![Status::Open, Status::Passed, Status::Rejected].contains(&prop.status) {
        return Err(ContractError::NotOpen {});
    }

    // if they are not expired
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // store sender vote
    if info.sender == config.dao {
        BALLOTS.update(
            deps.storage,
            (proposal_id, &MultisigRole::Dao),
            |bal| match bal {
                Some(_) => Err(ContractError::AlreadyVoted {}),
                None => Ok(vote),
            },
        )?;
    } else {
        BALLOTS.update(
            deps.storage,
            (proposal_id, &MultisigRole::Manager),
            |bal| match bal {
                Some(_) => Err(ContractError::AlreadyVoted {}),
                None => Ok(vote),
            },
        )?;
    }

    // update vote tally
    prop.votes.add_vote(vote, DEFAULT_WEIGHT);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    prop.update_status(&env.block);
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    // dispatch all proposed messages
    Ok(Response::new()
        .add_messages(prop.msgs)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response<Empty>, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    if [Status::Executed, Status::Rejected, Status::Passed].contains(&prop.status) {
        return Err(ContractError::WrongCloseStatus {});
    }

    // Avoid closing of Passed due to expiration proposals
    if prop.current_status(&env.block) == Status::Passed {
        return Err(ContractError::WrongCloseStatus {});
    }

    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    max_voting_period: Duration,
) -> Result<Response<Empty>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    config.max_voting_period = max_voting_period;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("sender", info.sender)
        .add_attribute("max_voting_period", max_voting_period.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::MigrationError {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        }
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(deps, proposal_id, start_after, limit)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        threshold: cfg.threshold.to_response(cfg.total_weight),
        max_voting_period: cfg.max_voting_period,
        dao: cfg.dao,
        manager: cfg.manager,
    })
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    let status = prop.current_status(&env.block);
    let threshold = prop.threshold.to_response(prop.total_weight);

    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        msgs: prop.msgs,
        status,
        expires: prop.expires,
        deposit: prop.deposit,
        proposer: prop.proposer,
        threshold,
    })
}

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let proposals = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect::<StdResult<_>>()?;

    Ok(ProposalListResponse { proposals })
}

fn reverse_proposals(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(Bound::exclusive);

    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal(
    block: &BlockInfo,
    item: StdResult<(u64, Proposal)>,
) -> StdResult<ProposalResponse> {
    item.map(|(id, prop)| {
        let status = prop.current_status(block);
        let threshold = prop.threshold.to_response(prop.total_weight);
        ProposalResponse {
            id,
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status,
            deposit: prop.deposit,
            proposer: prop.proposer,
            expires: prop.expires,
            threshold,
        }
    })
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter = deps.api.addr_validate(&voter)?;
    let cfg = CONFIG.load(deps.storage)?;

    let ballot;
    if voter == cfg.dao {
        ballot = BALLOTS.may_load(deps.storage, (proposal_id, &MultisigRole::Dao))?;
    } else if voter == cfg.manager {
        ballot = BALLOTS.may_load(deps.storage, (proposal_id, &MultisigRole::Manager))?;
    } else {
        return Err(StdError::generic_err(format!(
            "Vote not found for: {}",
            voter
        )));
    }

    let vote = ballot.map(|vote| VoteInfo {
        proposal_id,
        vote,
        voter: voter.to_string(),
        weight: DEFAULT_WEIGHT,
    });

    Ok(VoteResponse { vote })
}

fn list_votes(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let votes = BALLOTS
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(voter, vote)| VoteInfo {
                proposal_id,
                vote,
                voter: voter.to_string(),
                weight: DEFAULT_WEIGHT,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoteListResponse { votes })
}
