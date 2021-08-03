use std::ops::Add;

use cosmwasm_std::{
    CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
    WasmMsg,
};
use cw_storage_plus::U64Key;

use crate::balances::query_balance_of;
use crate::error::ContractError;
use crate::msg::{ProposalState, StateResponse};
use crate::state::{
    ExecuteData, Proposal, Receipt, CONFIG, GOVERNANCE_SATE, LATEST_PROPOSAL_IDS, PROPOSALS,
    RECEIPTS,
};

const MIN_TITLE_LENGTH: usize = 4;
const MAX_TITLE_LENGTH: usize = 64;
const MIN_DESC_LENGTH: usize = 4;
const MAX_DESC_LENGTH: usize = 1024;
const MIN_LINK_LENGTH: usize = 12;
const MAX_LINK_LENGTH: usize = 128;

pub fn propose(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    link: Option<String>,
    execute_msgs: Option<Vec<ExecuteData>>,
) -> Result<Response, ContractError> {
    validate_title(&title)?;
    validate_description(&description)?;
    validate_link(&link)?;

    let mut response = Response::default();
    response.add_attribute("Action", "ProposalCreated");
    let mut state = GOVERNANCE_SATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let user_balance = query_balance_of(deps.as_ref(), _env.clone(), info.sender.clone()).unwrap();
    if user_balance < config.proposal_weight {
        return Err(ContractError::proposal_err(format!(
            "Must proposal weight more than {} token",
            config.proposal_weight
        )));
    }
    let latest_proposal_id = LATEST_PROPOSAL_IDS
        .load(deps.storage, &info.sender)
        .unwrap_or(0);
    if latest_proposal_id != 0 {
        let proposers_latest_proposal_state =
            query_get_state(deps.as_ref(), _env.clone(), latest_proposal_id)
                .unwrap()
                .state;
        if proposers_latest_proposal_state == ProposalState::Active {
            return Err(ContractError::proposal_err(
                "Governor propose one live proposal per proposer, found an already active proposal",
            ));
        }
        if proposers_latest_proposal_state == ProposalState::Pending {
            return Err(ContractError::proposal_err("Governor propose one live proposal per proposer, found an already pending proposal"));
        }
    }
    let start_block = _env.block.height + config.voting_delay_period;
    let end_block = start_block + config.voting_period;
    state.proposal_count += 1;
    let mut data_list: Vec<ExecuteData> = vec![];
    let all_execute_data = if let Some(exe_msgs) = execute_msgs {
        for msgs in exe_msgs {
            let execute_data = ExecuteData {
                order: msgs.order,
                contract: deps.api.addr_validate(msgs.contract.as_str())?,
                msg: msgs.msg,
            };
            data_list.push(execute_data)
        }
        Some(data_list)
    } else {
        None
    };
    let new_proposal = Proposal {
        id: state.proposal_count,
        proposer: info.sender.clone(),
        eta: 0,
        title,
        description,
        link,
        execute_data: all_execute_data,
        start_block,
        end_block,
        for_votes: Uint128::zero(),
        against_votes: Uint128::zero(),
        canceled: false,
        executed: false,
    };
    let key = state.proposal_count;
    PROPOSALS
        .save(deps.storage, U64Key::from(key), &new_proposal)
        .unwrap();
    LATEST_PROPOSAL_IDS
        .save(deps.storage, &new_proposal.proposer, &new_proposal.id)
        .unwrap();
    response.add_attribute("id", new_proposal.id.to_string());
    response.add_attribute("proposer", info.sender.to_string());
    response.add_attribute("endBlock", end_block.to_string());
    GOVERNANCE_SATE.save(deps.storage, &state)?;
    Ok(response)
}

pub fn cast_vote(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    proposal_id: u64,
    support: bool,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "VoteCast");
    let state = query_get_state(deps.as_ref(), _env.clone(), proposal_id);
    if state.is_err() {
        return match state {
            Err(StdError::GenericErr { msg, .. }) => Err(ContractError::proposal_err(msg)),
            _ => Err(ContractError::proposal_err("oops...")),
        };
    }
    let proposers_proposal_state = state.unwrap().state;
    if proposers_proposal_state != ProposalState::Active {
        return Err(ContractError::ProposalError {
            msg: String::from("voting is closed"),
        });
    }
    let receipt = RECEIPTS
        .load(deps.storage, (U64Key::from(proposal_id), &info.sender))
        .unwrap_or(Receipt {
            has_voted: false,
            support,
            votes: Uint128::zero(),
        });
    if receipt.has_voted {
        return Err(ContractError::ProposalError {
            msg: String::from("voter already voted"),
        });
    }
    let vote_power = query_balance_of(deps.as_ref(), _env, info.sender.clone()).unwrap();
    PROPOSALS.update(
        deps.storage,
        U64Key::from(proposal_id),
        |prop| -> StdResult<_> {
            let mut val = prop.unwrap();
            if support {
                val.for_votes = val.for_votes.add(vote_power);
            } else {
                val.against_votes = val.against_votes.add(vote_power);
            }
            Ok(val)
        },
    )?;
    RECEIPTS.update(
        deps.storage,
        (U64Key::from(proposal_id), &info.sender),
        |res| -> StdResult<_> {
            let mut val = res.unwrap_or(Receipt {
                has_voted: true,
                support,
                votes: vote_power,
            });
            val.has_voted = true;
            val.support = support;
            val.votes = vote_power;
            Ok(val)
        },
    )?;
    response.add_attribute("proposal_id", proposal_id.to_string());
    response.add_attribute("vote_power", vote_power.to_string());
    response.add_attribute("voter", info.sender.to_string());
    response.add_attribute("support", support.to_string());
    Ok(response)
}

pub fn try_queue(deps: DepsMut, _env: Env, proposal_id: u64) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "ProposalQueued");
    let proposers_latest_proposal_state = query_get_state(deps.as_ref(), _env.clone(), proposal_id)
        .unwrap()
        .state;
    if proposers_latest_proposal_state != ProposalState::Succeeded {
        return Err(ContractError::proposal_err(
            "Governor queue proposal can only be queued if it is succeeded",
        ));
    }
    let config = CONFIG.load(deps.storage)?;
    let eta = _env.block.height + config.timelock_period;
    PROPOSALS.update(
        deps.storage,
        U64Key::from(proposal_id),
        |prop| -> StdResult<_> {
            let mut val = prop.unwrap();
            val.eta = eta;
            Ok(val)
        },
    )?;
    response.add_attribute("proposalId", proposal_id.to_string());
    response.add_attribute("Eta", eta.to_string());
    Ok(response)
}

pub fn try_execute(deps: DepsMut, _env: Env, proposal_id: u64) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "ProposalExecute");
    let proposers_proposal_state = query_get_state(deps.as_ref(), _env.clone(), proposal_id)
        .unwrap()
        .state;
    if proposers_proposal_state != ProposalState::Queued {
        return Err(ContractError::proposal_err(
            "queue proposal can only be queued if it is succeeded",
        ));
    }
    let config = CONFIG.load(deps.storage)?;
    let proposal = PROPOSALS
        .load(deps.storage, U64Key::from(proposal_id))
        .unwrap();
    if proposal.end_block + config.timelock_period > _env.block.height {
        return Err(ContractError::proposal_err(
            "Timelock period has not expired",
        ));
    }
    if let Some(all_msgs) = proposal.execute_data {
        let mut msgs = all_msgs;
        msgs.sort();
        for msg in msgs {
            response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: msg.contract.to_string(),
                msg: msg.msg,
                send: vec![],
            }))
        }
    } else {
        return Err(ContractError::ProposalError {
            msg: String::from("The poll does not have execute_data"),
        });
    }
    PROPOSALS.update(
        deps.storage,
        U64Key::from(proposal_id),
        |prop| -> StdResult<_> {
            let mut val = prop.unwrap();
            val.executed = true;
            Ok(val)
        },
    )?;
    response.add_attribute("proposalId", proposal_id.to_string());
    Ok(response)
}

pub fn try_cancel(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "ProposalCanceled");
    let config = CONFIG.load(deps.storage)?;
    let proposers_proposal_state = query_get_state(deps.as_ref(), _env.clone(), proposal_id)
        .unwrap()
        .state;
    if proposers_proposal_state != ProposalState::Executed {
        return Err(ContractError::proposal_err(
            "Governor cancel cannot cancel executed proposal",
        ));
    }
    let mut proposal = PROPOSALS
        .load(deps.storage, U64Key::from(proposal_id))
        .unwrap();
    if info.sender != config.admin
        || query_balance_of(deps.as_ref(), _env, info.sender).unwrap() <= config.proposal_weight
    {
        return Err(ContractError::ProposalError {
            msg: String::from("Governor cancel proposer above threshold"),
        });
    }
    proposal.canceled = true;
    PROPOSALS.update(
        deps.storage,
        U64Key::from(proposal_id),
        |prop| -> StdResult<_> {
            let mut val = prop.unwrap();
            val.canceled = true;
            Ok(val)
        },
    )?;
    response.add_attribute("proposalId", proposal_id.to_string());
    Ok(response)
}

pub fn query_get_state(deps: Deps, _env: Env, proposal_id: u64) -> StdResult<StateResponse> {
    let state = GOVERNANCE_SATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    if state.proposal_count < proposal_id || proposal_id == 0 {
        return Err(StdError::generic_err("state invalid proposal id"));
    }
    let proposal = PROPOSALS
        .load(deps.storage, U64Key::from(proposal_id))
        .unwrap();
    let yes = proposal.for_votes.u128();
    let no = proposal.against_votes.u128();
    let quorum = Decimal::from_ratio(yes + no, state.supply.u128());
    // let ratio = if yes+no == 0 {
    //     Decimal::zero()
    // }else{
    //     Decimal::from_ratio(yes, yes+no)
    // };
    // println!( "yes:{}, no:{} ratio:{} treshhold: {}", yes.to_string(), no.to_string(), ratio.to_string(), config.threshold.to_string());
    // println!("quorum: {}, config: {}", quorum.to_string(), config.quorum.to_string());
    // println!("time: {}, eta:{} exp: {}", _env.block.time.nanos().div(1_000_000_000).to_string(), proposal.eta.to_string(), config.expiration_period.to_string());
    if proposal.canceled {
        Ok(StateResponse {
            state: ProposalState::Canceled,
        })
    } else if _env.block.height <= proposal.start_block {
        Ok(StateResponse {
            state: ProposalState::Pending,
        })
    } else if _env.block.height <= proposal.end_block {
        Ok(StateResponse {
            state: ProposalState::Active,
        })
    } else if yes + no == 0
        || Decimal::from_ratio(yes, yes + no) < config.threshold
        || quorum < config.quorum
    {
        Ok(StateResponse {
            state: ProposalState::Defeated,
        })
    } else if proposal.eta == 0 {
        Ok(StateResponse {
            state: ProposalState::Succeeded,
        })
    } else if proposal.executed {
        Ok(StateResponse {
            state: ProposalState::Executed,
        })
        //} else if _env.block.time.nanos().div(1_000_000_000) >= proposal.eta + config.expiration_period {
    } else if _env.block.height >= proposal.eta + config.expiration_period {
        Ok(StateResponse {
            state: ProposalState::Expired,
        })
    } else {
        Ok(StateResponse {
            state: ProposalState::Queued,
        })
    }
}

pub fn query_get_proposal(deps: Deps, proposal_id: u64) -> StdResult<Option<Proposal>> {
    let proposal = PROPOSALS
        .load(deps.storage, U64Key::from(proposal_id))
        .unwrap();
    Ok(Option::from(proposal))
}

// validate_title returns an error if the title is invalid
fn validate_title(title: &str) -> Result<(), ContractError> {
    if title.len() < MIN_TITLE_LENGTH {
        Err(ContractError::proposal_err("Title too short"))
    } else if title.len() > MAX_TITLE_LENGTH {
        Err(ContractError::proposal_err("Title too long"))
    } else {
        Ok(())
    }
}

// validate_description returns an error if the description is invalid
fn validate_description(description: &str) -> Result<(), ContractError> {
    if description.len() < MIN_DESC_LENGTH {
        Err(ContractError::proposal_err("Description too short"))
    } else if description.len() > MAX_DESC_LENGTH {
        Err(ContractError::proposal_err("Description too long"))
    } else {
        Ok(())
    }
}

// validate_link returns an error if the link is invalid
fn validate_link(link: &Option<String>) -> Result<(), ContractError> {
    if let Some(link) = link {
        if link.len() < MIN_LINK_LENGTH {
            Err(ContractError::proposal_err("Link too short"))
        } else if link.len() > MAX_LINK_LENGTH {
            Err(ContractError::proposal_err("Link too long"))
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}
