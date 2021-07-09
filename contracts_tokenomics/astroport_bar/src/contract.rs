use cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, CanonicalAddr, CosmosMsg, InitResponse, HumanAddr};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::state::{State, CONFIG, Config};
use cw2::set_contract_version;
use cw20::MinterResponse;

use terraswap::token::InitMsg;
use terraswap::hook::InitHook;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-bar";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Store config
    CONFIG.save(deps.storage, &Config {
        token_code_id: msg.token_code_id,
        deposit_token_addr: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
    })?;

    // Create token
    let mut messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
        admin: None,
        code_id: msg.token_code_id,
        msg: to_binary(&InitMsg {
            name: "terraswap liquidity token".to_string(),
            symbol: "uLP".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: env.contract.address.to_string(),
                cap: None,
            }),
            init_hook: Some(InitHook {
                msg: to_binary(&ExecuteMsg::PostInitialize {})?,
                contract_addr: HumanAddr(env.contract.address.to_string())
            }),
        })?,
        send: vec![],
        label: String::from("Astroport Bar Token"),
    })];

    Ok(Response {
        submessages: vec![],
        messages,
        attributes: vec![],
        data: None
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::PostInitialize {} => try_post_initialize(deps, env),
        ExecuteMsg::Enter { amount } => try_enter(deps, env, info, amount),
        ExecuteMsg::Leave { share } => try_leave(deps, env, info, share),
    }
}


pub fn try_post_initialize(
    deps: DepsMut,
    env: Env,
) ->  StdResult<Response> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    // permission check
    if config.share_token_addr != CanonicalAddr::default() {
        return Err(StdError::unauthorized());
    }
    
    config.share_token_addr = deps.api.addr_canonicalize(&env.message.sender)?;

    CONFIG.save(deps.storage, &config)?;
    
    Ok(Response {
        submessages: vec![],
        messages: vec![],
        data: None,
        attributes: vec![]
    })
}

pub fn try_enter(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) ->  StdResult<Response> {

    let trs_token = STATE.load(deps.storage)?.trs_token;

    let total_trs = deps
        .querier
        .query_wasm_smart::<BalanceResponse, _, _>(
            trs_token.clone(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?
        .balance;

    let total_shares = TOKEN_INFO.load(deps.storage)?.total_supply;
    if total_shares.is_zero() || total_trs.is_zero() {
        execute_mint(
            deps,
            env.clone(),
            info.clone(),
            info.sender.to_string(),
            amount,
        )?;
    } else {
        let what = amount
            .checked_mul(total_shares)
            .map_err(StdError::overflow)?
            .checked_div(total_trs)
            .map_err(StdError::divide_by_zero)?;
        execute_mint(
            deps,
            env.clone(),
            info.clone(),
            info.sender.to_string(),
            what,
        )?;
    };

    Ok(Response {
        submessages: vec![],
        messages: vec![WasmMsg::Execute {
            contract_addr: trs_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: env.contract.address.to_string(),
                amount,
            })?,
            send: vec![],
        }
        .into()],
        attributes: vec![],
        data: None,
    })
}

pub fn try_leave(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    share: Uint128,
) ->  StdResult<Response> {
    let trs_token = STATE.load(deps.storage)?.trs_token;
    let total_trs = deps
        .querier
        .query_wasm_smart::<BalanceResponse, _, _>(
            trs_token.clone(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?
        .balance;

    let total_shares = TOKEN_INFO.load(deps.storage)?.total_supply;
    let what = share
        .checked_mul(total_trs)
        .map_err(StdError::overflow)?
        .checked_div(total_shares)
        .map_err(StdError::divide_by_zero)?;

    execute_burn_from(deps, env, info.clone(), info.sender.to_string(), share)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![WasmMsg::Execute {
            contract_addr: trs_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: what,
            })?,
            send: vec![],
        }
        .into()],
        attributes: vec![],
        data: None,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {

}