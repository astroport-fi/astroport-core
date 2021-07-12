use cosmwasm_std::{entry_point, to_binary, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, CanonicalAddr, CosmosMsg, HumanAddr};

use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::state::{CONFIG, Config};
use cw2::set_contract_version;
use cw20::{MinterResponse, BalanceResponse, Cw20QueryMsg, TokenInfoResponse, Cw20ExecuteMsg};

use terraswap::token::InitMsg;
use terraswap::hook::InitHook;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-bar";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const TOKEN_NAME: &str = "astroport-bar-token";
const TOKEN_SYMBOL: &str = "xASTR";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Store config
    CONFIG.save(deps.storage, &Config {
        token_code_id: msg.token_code_id,
        deposit_token_addr: deps.api.addr_canonicalize(&msg.deposit_token_addr.to_string())?,
        share_token_addr: CanonicalAddr::from(vec![]),
    })?;

    // Create token
    let mut resp = Response::new();
    resp.add_message(CosmosMsg::Wasm(WasmMsg::Instantiate {
        admin: None,
        code_id: msg.token_code_id,
        msg: to_binary(&InitMsg {
            name: TOKEN_NAME.to_string(),
            symbol: TOKEN_SYMBOL.to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: HumanAddr::from(env.contract.address.to_string())?,
                cap: None,
            }),
            init_hook: Some(InitHook {
                msg: to_binary(&ExecuteMsg::PostInitialize {})?,
                contract_addr: HumanAddr::from(env.contract.address.to_string())?,
            }),
        })?,
        send: vec![],
        label: String::from("Astroport Bar Token"),
    }));

    Ok(resp)
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
    if config.share_token_addr != CanonicalAddr::from(vec![]) {
        return Err(StdError::unauthorized());
    }

    // Set token addr
    config.share_token_addr = deps.api.addr_canonicalize(&env.message.sender)?;

    CONFIG.save(deps.storage, &config)?;
    
    Ok(Response::new())
}

pub fn try_enter(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) ->  StdResult<Response> {
    let config: Config = CONFIG.load(deps.storage)?;

    let total_deposit = get_total_deposit(deps.clone(), env, config.clone());
    let total_shares = get_total_shares(deps, config.clone());

    // If no balance exists, mint it 1:1 to the amount put in
    let mint_amount: Uint128 = if total_shares.is_zero() || total_deposit.is_zero() {
        amount
    } else {
        amount.checked_mul(total_shares)?.checked_div(total_deposit)?;
    };

    let mut res = Response::new();
    res.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.share_token_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: info.sender.to_string(),
            amount: mint_amount,
        })?,
        send: vec![],
    }));

    res.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.deposit_token_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: info.sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount,
        })?,
        send: vec![],
    }));

    Ok(res)
}

pub fn try_leave(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    share: Uint128,
) ->  StdResult<Response> {
    let config: Config = CONFIG.load(deps.storage)?;

    let total_deposit = get_total_deposit(deps.clone(), env, config.clone());
    let total_shares = get_total_shares(deps, config.clone());

    let what = share.checked_mul(total_deposit)?.checked_div(total_shares)?;

    // Burn share
    let mut res = Response::new();
    res.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.share_token_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: share,
        })?,
        send: vec![],
    }));

    res.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.deposit_token_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount: what,
        })?,
        send: vec![],
    }));

    Ok(res)
}

pub fn get_total_shares(deps: DepsMut, config: Config) -> Uint128{
    return deps.querier.query_wasm_smart::<TokenInfoResponse, _, _>(
        config.share_token_addr.clone(),
        &Cw20QueryMsg::TokenInfo {
        },
    ).unwrap().total_supply;
}

pub fn get_total_deposit(deps: DepsMut, env: Env, config: Config) -> Uint128 {
    return deps.querier.query_wasm_smart::<BalanceResponse, _, _>(
        config.deposit_token_addr.clone(),
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    ).unwrap().balance;
}