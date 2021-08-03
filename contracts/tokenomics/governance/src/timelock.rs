use crate::error::ContractError;
use crate::state::CONFIG;
use cosmwasm_std::{Addr, Decimal, DepsMut, Env, MessageInfo, Response, Uint128};

pub const MINIMUM_DELAY: u64 = 2 * 86400; // 2days
pub const MAXIMUM_DELAY: u64 = 30 * 86400; //30 days

pub fn try_timelock_period(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    timelock_period: u64,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "NewTimelockPeriod");
    if info.sender == _env.contract.address {
        return Err(ContractError::TimelockError {
            fun_name: String::from("setTimelockPeriod"),
            msg: String::from("Call must come from Timelock."),
        });
    }
    if timelock_period < MINIMUM_DELAY {
        return Err(ContractError::TimelockError {
            fun_name: String::from("setTimelockPeriod"),
            msg: String::from("Delay must exceed minimum delay"),
        });
    }
    if timelock_period > MAXIMUM_DELAY {
        return Err(ContractError::TimelockError {
            fun_name: String::from("setTimelockPeriod"),
            msg: String::from("Delay must not exceed maximum delay"),
        });
    }
    let mut config = CONFIG.load(deps.storage)?;
    config.timelock_period = timelock_period;
    CONFIG.save(deps.storage, &config)?;
    response.add_attribute("timelockPeriod", timelock_period.to_string());
    Ok(response)
}

pub fn try_accept_admin(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut response = Response::default();
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.pending_admin {
        return Err(ContractError::TimelockError {
            fun_name: String::from("acceptAdmin"),
            msg: String::from("Call must come from pendingAdmin"),
        });
    }
    config.admin = info.sender.clone();
    config.pending_admin = Addr::unchecked("0");
    CONFIG.save(deps.storage, &config)?;
    response.add_attribute("NewAdmin", info.sender.to_string());
    Ok(response)
}

pub fn try_set_pending_admin(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    admin: Addr,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("NewPendingAdmin", admin.to_string());
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    config.pending_admin = admin;
    CONFIG.save(deps.storage, &config)?;
    Ok(response)
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    guardian: Option<Addr>,
    timelock_period: Option<u64>,
    expiration_period: Option<u64>,
    quorum: Option<Decimal>,
    voting_period: Option<u64>,
    voting_delay_period: Option<u64>,
    threshold: Option<Decimal>,
    proposal_weight: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "UpdateConfig");
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(threshold) = threshold {
        config.threshold = threshold;
        response.add_attribute("threshold", config.threshold.to_string());
    }

    if let Some(quorum) = quorum {
        config.quorum = quorum;
        response.add_attribute("quorum", config.quorum.to_string());
    }
    if let Some(guardian) = guardian {
        config.guardian = guardian;
    }
    if let Some(voting_period) = voting_period {
        config.voting_period = voting_period;
    }

    if let Some(timelock_period) = timelock_period {
        config.timelock_period = timelock_period;
    }

    if let Some(expiration_period) = expiration_period {
        config.expiration_period = expiration_period;
    }

    if let Some(proposal_weight) = proposal_weight {
        config.proposal_weight = proposal_weight;
    }

    if let Some(voting_delay_period) = voting_delay_period {
        config.voting_delay_period = voting_delay_period;
    }
    CONFIG.save(deps.storage, &config)?;
    Ok(response)
}
