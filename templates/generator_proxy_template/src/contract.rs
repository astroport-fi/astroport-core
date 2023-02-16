// Delete this after changing all todo macros
#![allow(unused_variables, unreachable_code, clippy::diverging_sub_expression)]

use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::generator_proxy::{
    CallbackMsg, ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use cw2::set_contract_version;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-generator-proxy-template";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters (in [`InstantiateMsg`]).
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        generator_contract_addr: deps.api.addr_validate(&msg.generator_contract_addr)?,
        pair_addr: deps.api.addr_validate(&msg.pair_addr)?,
        lp_token_addr: deps.api.addr_validate(&msg.lp_token_addr)?,
        reward_contract_addr: deps.api.addr_validate(&msg.reward_contract_addr)?,
        reward_token_addr: deps.api.addr_validate(&msg.reward_token_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::UpdateRewards {}** Withdraw pending 3rd party rewards from the 3rd party staking contract.
///
/// * **ExecuteMsg::SendRewards { account, amount }** Sends accrued rewards to the recipient.
///
/// * **ExecuteMsg::Withdraw { account, amount }** Withdraw LP tokens and claim pending rewards.
///
/// * **ExecuteMsg::EmergencyWithdraw { account, amount }** Withdraw LP tokens without caring about pending rewards.
///
/// * **ExecuteMsg::Callback(msg)** Handles callbacks described in the [`CallbackMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, info, msg),
        ExecuteMsg::UpdateRewards {} => update_rewards(deps, info),
        ExecuteMsg::SendRewards { account, amount } => send_rewards(deps, info, account, amount),
        ExecuteMsg::Withdraw { account, amount } => withdraw(deps, env, info, account, amount),
        ExecuteMsg::EmergencyWithdraw { account, amount } => {
            emergency_withdraw(deps, env, info, account, amount)
        }
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;

    if let Ok(Cw20HookMsg::Deposit {}) = from_binary(&cw20_msg.msg) {
        if cw20_msg.sender != cfg.generator_contract_addr || info.sender != cfg.lp_token_addr {
            return Err(ContractError::Unauthorized {});
        }
        todo!("Deposit tokens in the end reward contract here");
    } else {
        return Err(ContractError::IncorrectCw20HookMessageVariant {});
    }
    Ok(response)
}

/// Withdraw pending rewards.
fn update_rewards(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.generator_contract_addr {
        return Err(ContractError::Unauthorized {});
    };

    response
        .messages
        .push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.reward_contract_addr.to_string(),
            funds: vec![],
            msg: todo!("Specify a withdraw rewards message that withdraws rewards from the end reward contract here"),
        })));

    Ok(response)
}

/// Sends rewards to a recipient.
///
/// * **account** account that receives the rewards.
///
/// * **amount** amount of rewards to send.
///
/// ## Executor
/// Only the Generator contract can execute this.
fn send_rewards(
    deps: DepsMut,
    info: MessageInfo,
    account: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    deps.api.addr_validate(&account)?;

    let mut response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.generator_contract_addr {
        return Err(ContractError::Unauthorized {});
    };

    response
        .messages
        .push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.reward_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: account,
                amount,
            })?,
            funds: vec![],
        })));
    Ok(response)
}

/// Withdraws/unstakes LP tokens and claims pending rewards.
///
/// * **account** account for which we withdraw LP tokens and claim rewards.
///
/// * **amount** amount of LP tokens to withdraw.
///
/// ## Executor
/// Only the Generator contract can execute this.
fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    account: String,
    _amount: Uint128,
) -> Result<Response, ContractError> {
    let account = deps.api.addr_validate(&account)?;

    let response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.generator_contract_addr {
        return Err(ContractError::Unauthorized {});
    };

    let prev_lp_balance = {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            &cfg.lp_token_addr,
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        res.balance
    };

    todo!("Withdraw from the 3rd party reward contract here");

    response.messages.push(SubMsg::new(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::Callback(
            CallbackMsg::TransferLpTokensAfterWithdraw {
                account,
                prev_lp_balance,
            },
        ))?,
    }));

    Ok(response)
}

/// Withdraw/unstakes LP tokens without withdrawing rewards.
///
/// * **account** account for which we withdraw LP tokens.
///
/// * **amount** amount of LP tokens to withdraw.
///
/// ## Executor
/// Only the Generator contract can execute this.
fn emergency_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    account: String,
    _amount: Uint128,
) -> Result<Response, ContractError> {
    let account = deps.api.addr_validate(&account)?;

    let response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.generator_contract_addr {
        return Err(ContractError::Unauthorized {});
    };

    let prev_lp_balance = {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            &cfg.lp_token_addr,
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        res.balance
    };

    todo!("Emergency withdraw from the 3rd party rewards contract here");

    response.messages.push(SubMsg::new(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::Callback(
            CallbackMsg::TransferLpTokensAfterWithdraw {
                account,
                prev_lp_balance,
            },
        ))?,
    }));

    Ok(response)
}

/// Handles callbacks described in [`CallbackMsg`].
///
/// ## Executor
/// Callback functions can only be called by this contract.
pub fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called by this contract
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::TransferLpTokensAfterWithdraw {
            account,
            prev_lp_balance,
        } => transfer_lp_tokens_after_withdraw(deps, env, account, prev_lp_balance),
    }
}

/// Transfers LP tokens after withdrawal (from the 3rd party staking contract) to a recipient.
///
/// * **account** account that receives the LP tokens.
///
/// * **prev_lp_balance** previous total amount of LP tokens that were being staked.
/// It is used for calculating the withdrawal amount.
pub fn transfer_lp_tokens_after_withdraw(
    deps: DepsMut,
    env: Env,
    account: Addr,
    prev_lp_balance: Uint128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let amount = {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            &cfg.lp_token_addr,
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        res.balance - prev_lp_balance
    };

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: cfg.lp_token_addr.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: account.to_string(),
            amount,
        })?,
    }))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Deposit {}** Returns the total amount of deposited LP tokens.
///
/// * **QueryMsg::Reward {}** Returns the total amount of reward tokens.
///
/// * **QueryMsg::PendingToken {}** Returns the total amount of pending rewards.
///
/// * **QueryMsg::RewardInfo {}** Returns the reward token contract address.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let cfg = CONFIG.load(deps.storage)?;
    match msg {
        QueryMsg::Config {} => to_binary(&ConfigResponse {
            generator_contract_addr: cfg.generator_contract_addr.to_string(),
            pair_addr: cfg.pair_addr.to_string(),
            lp_token_addr: cfg.lp_token_addr.to_string(),
            reward_contract_addr: cfg.reward_contract_addr.to_string(),
            reward_token_addr: cfg.reward_token_addr.to_string(),
        }),
        QueryMsg::Deposit {} => {
            todo!(
                "Query the 3rd party reward contract to retrieve the amount of staked LP tokens
            or implement local storage and retrieve from it here.
            The returned value must be a Uint128"
            );
        }
        QueryMsg::Reward {} => {
            let res: BalanceResponse = deps.querier.query_wasm_smart(
                cfg.reward_token_addr,
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.into_string(),
                },
            )?;
            let reward_amount = res.balance;

            to_binary(&reward_amount)
        }
        QueryMsg::PendingToken {} => {
            todo!(
                "Query the 3rd party reward contract and retrieve the pending token amount here.
            the returned value must be a Uint128"
            );
        }
        QueryMsg::RewardInfo {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&config.reward_token_addr)
        }
    }
}

/// Manages contract migration
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
