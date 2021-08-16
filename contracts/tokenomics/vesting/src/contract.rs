use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, ReplyOn,
    Response, StdResult, SubMsg, Timestamp, Uint128, WasmMsg,
};

use crate::state::{read_vesting_infos, Config, CONFIG, VESTING_INFO};

use crate::error::ContractError;
use astroport::vesting::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, OrderBy, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo,
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
            owner: deps.api.addr_canonicalize(msg.owner.as_str())?,
            token_addr: deps.api.addr_canonicalize(msg.token_addr.as_str())?,
            genesis_time: msg.genesis_time,
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
    match msg.clone() {
        ExecuteMsg::Claim {} => claim(deps, env, info),
        _ => {
            assert_owner_privilege(deps.as_ref(), env, info)?;
            match msg {
                ExecuteMsg::UpdateConfig {
                    owner,
                    token_addr,
                    genesis_time,
                } => update_config(deps, owner, token_addr, genesis_time),
                ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
                    register_vesting_accounts(deps, vesting_accounts)
                }
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
    }
}

fn assert_owner_privilege(deps: Deps, _env: Env, info: MessageInfo) -> Result<(), ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(ContractError::Unauthorized {});
    }

    Ok(())
}

pub fn update_config(
    deps: DepsMut,
    owner: Option<Addr>,
    token_addr: Option<Addr>,
    genesis_time: Option<Timestamp>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;
    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(owner.as_str())?;
    }

    if let Some(token_addr) = token_addr {
        config.token_addr = deps.api.addr_canonicalize(token_addr.as_str())?;
    }

    if let Some(genesis_time) = genesis_time {
        config.genesis_time = genesis_time;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

fn assert_vesting_schedules(
    vesting_schedules: &[(Timestamp, Timestamp, Uint128)],
) -> Result<(), ContractError> {
    for vesting_schedule in vesting_schedules.iter() {
        if vesting_schedule.0 >= vesting_schedule.1 {
            return Err(ContractError::EndTimeError {});
        }
    }

    Ok(())
}

pub fn register_vesting_accounts(
    deps: DepsMut,
    vesting_accounts: Vec<VestingAccount>,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    for vesting_account in vesting_accounts.iter() {
        assert_vesting_schedules(&vesting_account.schedules)?;

        VESTING_INFO.save(
            deps.storage,
            vesting_account.address.to_string(),
            &VestingInfo {
                last_claim_time: config.genesis_time,
                schedules: vesting_account.schedules.clone(),
            },
        )?;
    }

    Ok(Response::new().add_attribute("action", "register_vesting_accounts"))
}

pub fn claim(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let current_time = env.block.time;
    let address = info.sender;

    let config: Config = CONFIG.load(deps.storage)?;

    let mut vesting_info: VestingInfo = VESTING_INFO.load(deps.storage, address.to_string())?;

    let claim_amount = compute_claim_amount(current_time, &vesting_info);
    let messages: Vec<SubMsg> = if claim_amount.is_zero() {
        vec![]
    } else {
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.token_addr)?.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: address.to_string(),
                    amount: claim_amount,
                })?,
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }]
    };

    vesting_info.last_claim_time = current_time;
    VESTING_INFO.save(deps.storage, address.to_string(), &vesting_info)?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![
            attr("action", "claim"),
            attr("address", address),
            attr("claim_amount", claim_amount),
            attr("last_claim_time", current_time.seconds().to_string()),
        ]))
}

fn compute_claim_amount(current_time: Timestamp, vesting_info: &VestingInfo) -> Uint128 {
    let mut claimable_amount: Uint128 = Uint128::zero();
    for s in vesting_info.schedules.iter() {
        if s.0 > current_time || s.1 < vesting_info.last_claim_time {
            continue;
        }

        // min(s.1, current_time) - max(s.0, last_claim_time)
        let passed_time = std::cmp::min(s.1, current_time).seconds()
            - std::cmp::max(s.0, vesting_info.last_claim_time).seconds();

        // prevent zero time_period case
        let time_period = s.1.seconds() - s.0.seconds();
        let release_amount_per_time: Decimal = Decimal::from_ratio(s.2, time_period);

        claimable_amount += Uint128::new(passed_time as u128) * release_amount_per_time;
    }

    claimable_amount
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
        owner: deps.api.addr_humanize(&config.owner)?,
        token_addr: deps.api.addr_humanize(&config.token_addr)?,
        genesis_time: config.genesis_time,
    };

    Ok(resp)
}

pub fn query_vesting_account(deps: Deps, address: Addr) -> StdResult<VestingAccountResponse> {
    let info: VestingInfo = VESTING_INFO.load(deps.storage, address.to_string())?;

    let resp = VestingAccountResponse { address, info };

    Ok(resp)
}

pub fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let vesting_infos = if let Some(start_after) = start_after {
        read_vesting_infos(
            deps,
            Some(deps.api.addr_canonicalize(start_after.as_str())?),
            limit,
            order_by,
        )?
    } else {
        read_vesting_infos(deps, None, limit, order_by)?
    };

    let vesting_account_responses: StdResult<Vec<VestingAccountResponse>> = vesting_infos
        .iter()
        .map(|vesting_account| {
            Ok(VestingAccountResponse {
                address: vesting_account.0.clone(),
                info: vesting_account.1.clone(),
            })
        })
        .collect();

    Ok(VestingAccountsResponse {
        vesting_accounts: vesting_account_responses?,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

#[test]
fn test_assert_vesting_schedules() {
    // valid
    assert_vesting_schedules(&vec![
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(101),
            Uint128::from(100u128),
        ),
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(110),
            Uint128::from(100u128),
        ),
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(200),
            Uint128::from(100u128),
        ),
    ])
    .unwrap();

    // invalid
    let res = assert_vesting_schedules(&vec![
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(100),
            Uint128::from(100u128),
        ),
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(110),
            Uint128::from(100u128),
        ),
        (
            Timestamp::from_seconds(100),
            Timestamp::from_seconds(200),
            Uint128::from(100u128),
        ),
    ])
    .unwrap_err();

    assert_eq!(res, ContractError::EndTimeError {});
}
