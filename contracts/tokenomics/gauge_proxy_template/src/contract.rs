use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::state::{Config, CONFIG, USER_INFO};
use gauge_proxy_interface::msg::{Cw20HookMsg, ExecuteMsg, QueryMsg};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        gauge_contract_addr: deps.api.addr_validate(&msg.gauge_contract_addr)?,
        lp_token_addr: deps.api.addr_validate(&msg.lp_token_addr)?,
        reward_contract_addr: deps.api.addr_validate(&msg.reward_contract_addr)?,
        reward_token_addr: deps.api.addr_validate(&msg.reward_token_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Withdraw { account, amount } => withdraw(deps, env, info, account, amount),
        ExecuteMsg::EmergencyWithdraw { account } => emergency_withdraw(deps, env, info, account),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let mut response = Response::default();

    if let Ok(Cw20HookMsg::Deposit { account }) = from_binary(&cw20_msg.msg) {
        if cw20_msg.sender != cfg.gauge_contract_addr || info.sender != cfg.lp_token_addr {
            return Err(ContractError::Unauthorized {});
        }

        let mut user = USER_INFO.load(deps.storage, &account).unwrap_or_default();

        let lp_new_balance = get_token_balance(deps.as_ref(), &env, &cfg.lp_token_addr)?;
        let rewards_balance = get_token_balance(deps.as_ref(), &env, &cfg.reward_token_addr)?;

        let lp_old_balance = lp_new_balance.checked_sub(cw20_msg.amount)?;
        if !user.amount.is_zero() && !lp_old_balance.is_zero() {
            let pending = user
                .amount
                .checked_mul(rewards_balance)?
                .checked_div(lp_old_balance)
                .map_err(StdError::from)?
                .checked_sub(user.reward_debt)
                .unwrap_or_default();
            if !pending.is_zero() {
                response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: cfg.reward_token_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: account.to_string(),
                        amount: pending.min(lp_old_balance),
                    })?,
                    funds: vec![],
                }));
            }
        }

        if !cw20_msg.amount.is_zero() {
            // stake to the end reward contract here
            unimplemented!();
        }

        user.amount = user.amount.checked_add(cw20_msg.amount)?;
        if !lp_old_balance.is_zero() {
            user.reward_debt = user
                .amount
                .checked_mul(rewards_balance)?
                .checked_div(lp_old_balance)
                .map_err(StdError::from)?;
        }
        USER_INFO.save(deps.storage, &account, &user)?;
    }
    Ok(response)
}

fn get_token_balance(deps: Deps, env: &Env, token: &Addr) -> Result<Uint128, StdError> {
    Ok(deps
        .querier
        .query_wasm_smart::<BalanceResponse, _, _>(
            token.clone(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?
        .balance)
}

fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    account: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.gauge_contract_addr {
        return Err(ContractError::Unauthorized {});
    };
    let mut response = Response::default();
    let mut user = USER_INFO.load(deps.storage, &account)?;

    let amount = user.amount.min(amount);

    let lp_balance = get_token_balance(deps.as_ref(), &env, &cfg.lp_token_addr)?;
    let rewards_balance = get_token_balance(deps.as_ref(), &env, &cfg.reward_token_addr)?;

    if !amount.is_zero() && !lp_balance.is_zero() {
        let pending = amount
            .checked_mul(rewards_balance)?
            .checked_div(lp_balance)
            .map_err(StdError::from)?
            .checked_sub(user.reward_debt)
            .unwrap_or_default();
        if !pending.is_zero() {
            response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cfg.reward_token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: account.to_string(),
                    amount: pending.min(lp_balance),
                })?,
                funds: vec![],
            }));
        }
    }

    user.amount = user.amount.checked_sub(amount)?;
    user.reward_debt = user
        .amount
        .checked_mul(rewards_balance)?
        .checked_div(lp_balance)
        .map_err(StdError::from)?;

    USER_INFO.save(deps.storage, &account, &user)?;

    // withdraw from the end reward contract here
    unimplemented!();

    Ok(response)
}

fn emergency_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    account: Addr,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.gauge_contract_addr {
        return Err(ContractError::Unauthorized {});
    };

    let mut response = Response::default();
    let user = USER_INFO.load(deps.storage, &account)?;

    // emergency withdraw from the end reward contract here
    unimplemented!();

    USER_INFO.remove(deps.storage, &account);
    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}
