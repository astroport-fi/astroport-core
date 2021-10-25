use std::cmp::Ordering;

use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, Event, MessageInfo,
    Response, StdError, StdResult, SubMsg, Timestamp, Uint128, WasmMsg,
};

use crate::state::{read_vesting_infos, Config, CONFIG, VESTING_INFO};

use crate::error::ContractError;
use astroport::vesting::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, OrderBy, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            token_addr: deps.api.addr_validate(&msg.token_addr)?,
        },
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Claim { recipient, amount } => claim(deps, env, info, recipient, amount),
        ExecuteMsg::UpdateConfig { owner } => update_config(deps, info, owner),
        ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, env, info, vesting_accounts)
        }
    }
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(&owner)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules.iter() {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.clone()));
            }
        }
    }

    Ok(())
}

pub fn register_vesting_accounts(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vesting_accounts: Vec<VestingAccount>,
) -> Result<Response, ContractError> {
    let mut response = Response::new();

    let mut event = Event::new("Register vesting accounts".to_string());

    let config: Config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let mut to_deposit = Uint128::zero();
    let mut to_receive = Uint128::zero();

    for vesting_account in vesting_accounts {
        let account_address = deps.api.addr_validate(&vesting_account.address)?;

        assert_vesting_schedules(&account_address, &vesting_account.schedules)?;

        for sch in &vesting_account.schedules {
            to_deposit += if let Some(end_point) = &sch.end_point {
                end_point.amount
            } else {
                sch.start_point.amount
            }
        }

        if let Some(old_info) = VESTING_INFO.may_load(deps.storage, &account_address)? {
            let mut total = Uint128::zero();
            for sch in &old_info.schedules {
                total += if let Some(end_point) = &sch.end_point {
                    end_point.amount
                } else {
                    sch.start_point.amount
                }
            }

            to_receive += total - old_info.released_amount;
        }

        VESTING_INFO.save(
            deps.storage,
            &account_address,
            &VestingInfo {
                schedules: vesting_account.schedules,
                released_amount: Uint128::zero(),
            },
        )?;
    }

    match to_deposit.cmp(&to_receive) {
        Ordering::Greater => {
            let amount = to_deposit - to_receive;
            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: config.token_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: config.owner.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount,
                })?,
            }));
            event.attributes.push(attr("deposited", amount));
        }
        Ordering::Less => {
            let amount = to_receive - to_deposit;
            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: config.token_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: config.owner.to_string(),
                    amount,
                })?,
            }));
            event.attributes.push(attr("received", amount));
        }
        Ordering::Equal => {}
    }

    Ok(response)
}

pub fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let mut event = Event::new("Claim".to_string()).add_attribute("address", info.sender.clone());

    let config: Config = CONFIG.load(deps.storage)?;

    let mut vesting_info: VestingInfo = VESTING_INFO.load(deps.storage, &info.sender)?;

    let available_amount = compute_available_amount(env.block.time, &vesting_info)?;

    let claim_amount = if let Some(a) = amount {
        if a > available_amount {
            return Err(ContractError::AmountIsNotAvailable {});
        };
        a
    } else {
        available_amount
    };

    event.attributes.append(&mut vec![
        attr("available_amount", available_amount),
        attr("claimed_amount", claim_amount),
    ]);

    if !claim_amount.is_zero() {
        response
            .messages
            .append(&mut vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: config.token_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.unwrap_or_else(|| info.sender.to_string()),
                    amount: claim_amount,
                })?,
            })]);

        vesting_info.released_amount = vesting_info.released_amount.checked_add(claim_amount)?;
        VESTING_INFO.save(deps.storage, &info.sender, &vesting_info)?;
    };

    Ok(response.add_event(event))
}

fn compute_available_amount(
    current_time: Timestamp,
    vesting_info: &VestingInfo,
) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in vesting_info.schedules.iter() {
        if sch.start_point.time > current_time {
            continue;
        }

        available_amount = available_amount.checked_add(sch.start_point.amount)?;

        if let Some(end_point) = &sch.end_point {
            let passed_time =
                current_time.min(end_point.time).seconds() - sch.start_point.time.seconds();
            let time_period = end_point.time.seconds() - sch.start_point.time.seconds();
            if passed_time != 0 && time_period != 0 {
                let release_amount_per_second: Decimal = Decimal::from_ratio(
                    end_point.amount.checked_sub(sch.start_point.amount)?,
                    time_period,
                );

                available_amount += Uint128::new(passed_time as u128) * release_amount_per_second;
            }
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::VestingAccount { address } => {
            Ok(to_binary(&query_vesting_account(deps, address)?)?)
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => Ok(to_binary(&query_vesting_accounts(
            deps,
            start_after,
            limit,
            order_by,
        )?)?),
        QueryMsg::AvailableAmount { address } => Ok(to_binary(&query_vesting_available_amount(
            deps, _env, address,
        )?)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        owner: config.owner,
        token_addr: config.token_addr,
    };

    Ok(resp)
}

pub fn query_vesting_account(deps: Deps, address: Addr) -> StdResult<VestingAccountResponse> {
    let info: VestingInfo = VESTING_INFO.load(deps.storage, &address)?;

    let resp = VestingAccountResponse { address, info };

    Ok(resp)
}

pub fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let vesting_infos = read_vesting_infos(deps, start_after, limit, order_by)?;

    let vesting_account_responses: Vec<VestingAccountResponse> = vesting_infos
        .into_iter()
        .map(|(address, info)| VestingAccountResponse { address, info })
        .collect();

    Ok(VestingAccountsResponse {
        vesting_accounts: vesting_account_responses,
    })
}

pub fn query_vesting_available_amount(deps: Deps, env: Env, address: Addr) -> StdResult<Uint128> {
    let info: VestingInfo = VESTING_INFO.load(deps.storage, &address)?;
    let available_amount = compute_available_amount(env.block.time, &info)?;
    Ok(available_amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
