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
use cw20::Cw20ExecuteMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
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
        ExecuteMsg::UpdateConfig { owner, token_addr } => {
            update_config(deps, info, owner, token_addr)
        }
        ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, info, vesting_accounts)
        }
    }
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    token_addr: Option<String>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(&owner)?;
    }

    if let Some(token_addr) = token_addr {
        config.token_addr = deps.api.addr_validate(&token_addr)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules.iter() {
        if !(sch.starts_at < sch.ends_at && sch.amount_at_start < sch.total_amount
            || sch.starts_at == sch.ends_at && sch.amount_at_start == sch.total_amount)
        {
            return Err(ContractError::VestingScheduleError(addr.clone()));
        }
    }

    Ok(())
}

pub fn register_vesting_accounts(
    deps: DepsMut,
    info: MessageInfo,
    vesting_accounts: Vec<VestingAccount>,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    for vesting_account in vesting_accounts {
        let account_address = deps.api.addr_validate(&vesting_account.address)?;

        assert_vesting_schedules(&account_address, &vesting_account.schedules)?;

        VESTING_INFO.save(
            deps.storage,
            &account_address,
            &VestingInfo {
                schedules: vesting_account.schedules,
                released_amount: Uint128::zero(),
            },
        )?;
    }

    Ok(Response::new().add_event(Event::new("Register vesting accounts".to_string())))
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
        if sch.starts_at > current_time {
            continue;
        }

        available_amount = available_amount.checked_add(sch.amount_at_start)?;

        let passed_time = current_time.min(sch.ends_at).seconds() - sch.starts_at.seconds();
        let time_period = sch.ends_at.seconds() - sch.starts_at.seconds();
        if passed_time != 0 && time_period != 0 {
            let release_amount_per_time: Decimal = Decimal::from_ratio(
                sch.total_amount.checked_sub(sch.amount_at_start)?,
                time_period,
            );

            available_amount += Uint128::new(passed_time as u128) * release_amount_per_time;
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
