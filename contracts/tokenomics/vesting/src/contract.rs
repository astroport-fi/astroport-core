use cosmwasm_std::{
    log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse, HandleResult,
    HumanAddr, InitResponse, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::state::{
    read_config, read_vesting_info, read_vesting_infos, store_config, store_vesting_info, Config
};

use terraswap::vesting::{
    ConfigResponse, HandleMsg, InitMsg, QueryMsg, VestingAccount, VestingAccountResponse,
    VestingAccountsResponse, VestingInfo, OrderBy
};
use cw20::Cw20HandleMsg;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    store_config(
        &mut deps.storage,
        &Config {
            owner: deps.api.canonical_address(&msg.owner)?,
            token_addr: deps.api.canonical_address(&msg.token_addr)?,
            genesis_time: msg.genesis_time,
        },
    )?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg.clone() {
        HandleMsg::Claim {} => claim(deps, env),
        _ => {
            assert_owner_privilege(deps, env.clone())?;
            match msg {
                HandleMsg::UpdateConfig {
                    owner,
                    token_addr,
                    genesis_time,
                } => update_config(deps, owner, token_addr, genesis_time),
                HandleMsg::RegisterVestingAccounts { vesting_accounts } => {
                    register_vesting_accounts(deps, vesting_accounts)
                }
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
    }
}

fn assert_owner_privilege<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<()> {
    if read_config(&deps.storage)?.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    Ok(())
}

pub fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    owner: Option<HumanAddr>,
    token_addr: Option<HumanAddr>,
    genesis_time: Option<u64>,
) -> HandleResult {
    let mut config = read_config(&deps.storage)?;
    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(token_addr) = token_addr {
        config.token_addr = deps.api.canonical_address(&token_addr)?;
    }

    if let Some(genesis_time) = genesis_time {
        config.genesis_time = genesis_time;
    }

    store_config(&mut deps.storage, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}

fn assert_vesting_schedules(vesting_schedules: &Vec<(u64, u64, Uint128)>) -> StdResult<()> {
    for vesting_schedule in vesting_schedules.iter() {
        if vesting_schedule.0 >= vesting_schedule.1 {
            return Err(StdError::generic_err(
                "end_time must bigger than start_time",
            ));
        }
    }

    return Ok(());
}

pub fn register_vesting_accounts<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    vesting_accounts: Vec<VestingAccount>,
) -> HandleResult {
    let config: Config = read_config(&deps.storage)?;
    for vesting_account in vesting_accounts.iter() {
        assert_vesting_schedules(&vesting_account.schedules)?;

        let vesting_address = deps.api.canonical_address(&vesting_account.address)?;
        store_vesting_info(
            &mut deps.storage,
            &vesting_address,
            &VestingInfo {
                last_claim_time: config.genesis_time,
                schedules: vesting_account.schedules.clone(),
            },
        )?;
    }

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "register_vesting_accounts")],
        data: None,
    })
}

pub fn claim<S: Storage, A: Api, Q: Querier>(deps: &mut Extern<S, A, Q>, env: Env) -> HandleResult {
    let current_time = env.block.time;
    let address = env.message.sender;
    let address_raw = deps.api.canonical_address(&address)?;

    let config: Config = read_config(&deps.storage)?;
    let mut vesting_info: VestingInfo = read_vesting_info(&deps.storage, &address_raw)?;

    let claim_amount = compute_claim_amount(current_time, &vesting_info);
    let messages: Vec<CosmosMsg> = if claim_amount.is_zero() {
        vec![]
    } else {
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.token_addr)?,
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: address.clone(),
                amount: claim_amount,
            })?,
        })]
    };

    vesting_info.last_claim_time = current_time;
    store_vesting_info(&mut deps.storage, &address_raw, &vesting_info)?;

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "claim"),
            log("address", address),
            log("claim_amount", claim_amount),
            log("last_claim_time", current_time),
        ],
        data: None,
    })
}

fn compute_claim_amount(current_time: u64, vesting_info: &VestingInfo) -> Uint128 {
    let mut claimable_amount: Uint128 = Uint128::zero();
    for s in vesting_info.schedules.iter() {
        if s.0 > current_time || s.1 < vesting_info.last_claim_time {
            continue;
        }

        // min(s.1, current_time) - max(s.0, last_claim_time)
        let passed_time =
            std::cmp::min(s.1, current_time) - std::cmp::max(s.0, vesting_info.last_claim_time);

        // prevent zero time_period case
        let time_period = s.1 - s.0;
        let release_amount_per_time: Decimal = Decimal::from_ratio(s.2, time_period);

        claimable_amount += Uint128(passed_time as u128) * release_amount_per_time;
    }

    return claimable_amount;
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
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

pub fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let state = read_config(&deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.human_address(&state.owner)?,
        token_addr: deps.api.human_address(&state.token_addr)?,
        genesis_time: state.genesis_time,
    };

    Ok(resp)
}

pub fn query_vesting_account<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<VestingAccountResponse> {
    let info = read_vesting_info(&deps.storage, &deps.api.canonical_address(&address)?)?;
    let resp = VestingAccountResponse { address, info };

    Ok(resp)
}

pub fn query_vesting_accounts<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<HumanAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let vesting_infos = if let Some(start_after) = start_after {
        read_vesting_infos(
            &deps.storage,
            Some(deps.api.canonical_address(&start_after)?),
            limit,
            order_by,
        )?
    } else {
        read_vesting_infos(&deps.storage, None, limit, order_by)?
    };

    let vesting_account_responses: StdResult<Vec<VestingAccountResponse>> = vesting_infos
        .iter()
        .map(|vesting_account| {
            Ok(VestingAccountResponse {
                address: deps.api.human_address(&vesting_account.0)?,
                info: vesting_account.1.clone(),
            })
        })
        .collect();

    Ok(VestingAccountsResponse {
        vesting_accounts: vesting_account_responses?,
    })
}

#[test]
fn test_assert_vesting_schedules() {
    // valid
    assert_vesting_schedules(&vec![
        (100u64, 101u64, Uint128::from(100u128)),
        (100u64, 110u64, Uint128::from(100u128)),
        (100u64, 200u64, Uint128::from(100u128)),
    ])
    .unwrap();

    // invalid
    let res = assert_vesting_schedules(&vec![
        (100u64, 100u64, Uint128::from(100u128)),
        (100u64, 110u64, Uint128::from(100u128)),
        (100u64, 200u64, Uint128::from(100u128)),
    ]);
    match res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "end_time must bigger than start_time")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }
}
