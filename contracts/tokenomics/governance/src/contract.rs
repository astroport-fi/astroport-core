use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128,
};

use crate::balances::{
    checkpoint, create_lock, deposit, increase_amount, increase_unlock_time, query_balance_of,
    query_balance_of_at, query_total_supply, query_total_supply_at, withdraw,
};
use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::proposal::{
    cast_vote, propose, query_get_proposal, query_get_state, try_cancel, try_execute, try_queue,
};
use crate::state::{
    Config, LockedBalance, State, CONFIG, GOVERNANCE_SATE, LOCKED, USER_POINT_EPOCH,
    USER_POINT_HISTORY,
};
use crate::timelock::{
    try_accept_admin, try_set_pending_admin, try_timelock_period, update_config, MAXIMUM_DELAY,
    MINIMUM_DELAY,
};

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    validate_quorum(msg.quorum)?;
    validate_threshold(msg.threshold)?;

    if msg.timelock_period < MINIMUM_DELAY {
        return Err(ContractError::TimelockError {
            fun_name: String::from("init"),
            msg: String::from("Delay must exceed minimum delay"),
        });
    }
    if msg.timelock_period > MAXIMUM_DELAY {
        return Err(ContractError::TimelockError {
            fun_name: String::from("init"),
            msg: String::from("Delay must not exceed maximum delay"),
        });
    }
    let state = State {
        proposal_count: 0,
        owner: info.sender,
        supply: Uint128::zero(),
        epoch: 0,
        point_history: Vec::with_capacity(4_000_000_000),
    };
    let config = Config {
        admin: msg.admin.clone(),
        guardian: msg.guardian,
        timelock_period: msg.timelock_period,
        pending_admin: msg.admin,
        threshold: msg.threshold,
        proposal_weight: msg.proposal_weight,
        xtrs_token: msg.token,
        quorum: msg.quorum,
        voting_period: msg.voting_period,
        voting_delay_period: msg.voting_delay_period,
        expiration_period: msg.expiration_period,
    };
    GOVERNANCE_SATE.save(deps.storage, &state)?;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            link,
            execute_data,
        } => propose(deps, _env, info, title, description, link, execute_data),
        ExecuteMsg::Vote {
            proposal_id,
            support,
        } => cast_vote(deps, _env, info, proposal_id, support),
        ExecuteMsg::Queue { proposal_id } => try_queue(deps, _env, proposal_id),
        ExecuteMsg::Execute { proposal_id } => try_execute(deps, _env, proposal_id),
        ExecuteMsg::Cancel { proposal_id } => try_cancel(deps, _env, info, proposal_id),

        ExecuteMsg::SetDelay { delay } => try_timelock_period(deps, _env, info, delay),
        ExecuteMsg::SetPendingAdmin { admin } => try_set_pending_admin(deps, _env, info, admin),
        ExecuteMsg::AcceptAdmin {} => try_accept_admin(deps, info),
        ExecuteMsg::UpdateGovernanceConfig {
            guardian,
            timelock_period,
            expiration_period,
            voting_period,
            voting_delay_period,
            threshold,
            quorum,
            proposal_weight,
        } => update_config(
            deps,
            info,
            guardian,
            timelock_period,
            expiration_period,
            quorum,
            voting_period,
            voting_delay_period,
            threshold,
            proposal_weight,
        ),

        ExecuteMsg::CreateLock { amount, lock } => create_lock(deps, _env, info, amount, lock),
        ExecuteMsg::IncreaseAmount { amount } => increase_amount(deps, _env, info, amount),
        ExecuteMsg::IncreaseUnlockTime { unlock_time } => {
            increase_unlock_time(deps, _env, info, unlock_time)
        }
        ExecuteMsg::Deposit { user, amount } => deposit(deps, _env, user, amount),
        ExecuteMsg::Withdraw {} => withdraw(deps, _env, info),
        ExecuteMsg::Checkpoint {} => checkpoint(deps, _env),
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState { proposal_id } => to_binary(&query_get_state(deps, _env, proposal_id)?),
        QueryMsg::GetProposal { proposal_id } => to_binary(&query_get_proposal(deps, proposal_id)?),
        QueryMsg::GetBalanceOf { user } => to_binary(&query_balance_of(deps, _env, user)?),
        QueryMsg::GetBalanceOfAt { user, block } => {
            to_binary(&query_balance_of_at(deps, _env, user, block)?)
        }
        QueryMsg::GetLockedBalance { user } => to_binary(&query_locked_balance(deps, user)?),
        QueryMsg::LockedEnd { user } => to_binary(&query_locked_end(deps, user)?),
        QueryMsg::GetTotalSupply {} => to_binary(&query_total_supply(deps, _env)?),
        QueryMsg::GetTotalSupplyAt { block } => {
            to_binary(&query_total_supply_at(deps, _env, block)?)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        guardian: config.guardian,
        quorum: config.quorum,
        threshold: config.threshold,
        voting_period: config.voting_period,
        timelock_period: config.timelock_period,
        expiration_period: config.expiration_period,
        proposal_weight: config.proposal_weight,
        voting_delay_period: config.voting_delay_period,
    })
}

pub fn query_locked_balance(deps: Deps, user: Addr) -> StdResult<LockedBalance> {
    LOCKED.load(deps.storage, &user)
}

pub fn query_locked_end(deps: Deps, user: Addr) -> StdResult<u64> {
    // Get timestamp when `user`'s lock finishes
    // param user User wallet
    // return Epoch time of the lock end
    Ok(LOCKED.load(deps.storage, &user).unwrap().end)
}

pub fn query_last_user_slope(deps: Deps, user: Addr) -> StdResult<Uint128> {
    // Get the most recently recorded rate of voting power decrease for `addr`
    // param addr Address of the user wallet
    // return Value of the slope

    let u_epoch = USER_POINT_EPOCH.load(deps.storage, &user).unwrap();
    let slopes = USER_POINT_HISTORY.load(deps.storage, &user).unwrap();
    Ok(slopes[u_epoch].slope)
}

// validate_quorum returns an error if the quorum is invalid
// (we require 0-1)
fn validate_quorum(quorum: Decimal) -> Result<(), ContractError> {
    if quorum > Decimal::one() {
        Err(ContractError::proposal_err("quorum must be 0 to 1"))
    } else {
        Ok(())
    }
}

// validate_threshold returns an error if the threshold is invalid
// (we require 0-1)
fn validate_threshold(threshold: Decimal) -> Result<(), ContractError> {
    if threshold > Decimal::one() {
        Err(ContractError::proposal_err("threshold must be 0 to 1"))
    } else {
        Ok(())
    }
}
