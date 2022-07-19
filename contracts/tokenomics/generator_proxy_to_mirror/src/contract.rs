use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::asset::addr_validate_to_lower;
use astroport::generator_proxy::{
    CallbackMsg, ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use cw2::set_contract_version;
use mirror_protocol::staking::{
    Cw20HookMsg as MirrorCw20HookMsg, ExecuteMsg as MirrorExecuteMsg, QueryMsg as MirrorQueryMsg,
    RewardInfoResponse,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-generator-proxy-to-mirror";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters (in [`InstantiateMsg`]).
/// Returns a default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        generator_contract_addr: addr_validate_to_lower(deps.api, &msg.generator_contract_addr)?,
        pair_addr: addr_validate_to_lower(deps.api, &msg.pair_addr)?,
        lp_token_addr: addr_validate_to_lower(deps.api, &msg.lp_token_addr)?,
        reward_contract_addr: addr_validate_to_lower(deps.api, &msg.reward_contract_addr)?,
        reward_token_addr: addr_validate_to_lower(deps.api, &msg.reward_token_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// ## Description
/// Exposes execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::UpdateRewards {}** Withdraw pending 3rd party rewards.
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
        ExecuteMsg::UpdateRewards {} => update_rewards(deps),
        ExecuteMsg::SendRewards { account, amount } => send_rewards(deps, info, account, amount),
        ExecuteMsg::Withdraw { account, amount } => withdraw(deps, env, info, account, amount),
        ExecuteMsg::EmergencyWithdraw { account, amount } => {
            withdraw(deps, env, info, account, amount)
        }
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;

    if let Ok(Cw20HookMsg::Deposit {}) = from_binary(&cw20_msg.msg) {
        if cw20_msg.sender != cfg.generator_contract_addr || info.sender != cfg.lp_token_addr {
            return Err(ContractError::Unauthorized {});
        }
        response
            .messages
            .push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cfg.lp_token_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: cfg.reward_contract_addr.to_string(),
                    amount: cw20_msg.amount,
                    msg: to_binary(&MirrorCw20HookMsg::Bond {
                        asset_token: cfg.pair_addr.into_string(),
                    })?,
                })?,
            })));
    } else {
        return Err(ContractError::IncorrectCw20HookMessageVariant {});
    }
    Ok(response)
}

/// ## Description
/// Withdraw pending rewards. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] object with the specified submessages.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
fn update_rewards(deps: DepsMut) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let cfg = CONFIG.load(deps.storage)?;

    response
        .messages
        .push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.reward_contract_addr.to_string(),
            funds: vec![],
            msg: to_binary(&MirrorExecuteMsg::Withdraw {
                asset_token: Some(cfg.pair_addr.into_string()),
            })?,
        })));

    Ok(response)
}

/// ## Description
/// Sends rewards to a recipient. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] object with the specified submessages.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **account** is an object of type [`String`]. This is the account that receives the rewards.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of rewards to send.
///
/// ## Executor
/// Only the Generator contract can execute this.
fn send_rewards(
    deps: DepsMut,
    info: MessageInfo,
    account: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    addr_validate_to_lower(deps.api, &account)?;

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

/// ## Description
/// Withdraws/unstakes LP tokens and claims pending rewards. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] object with the specified
/// submessages if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **account** is an object of type [`String`]. This is the account for which we withdraw LP tokens and claim rewards.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to withdraw.
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
    let account = addr_validate_to_lower(deps.api, &account)?;

    let mut response = Response::new();
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

    // withdraw from the end reward contract
    // TODO: mirror protocol still depends on cosmwasm 0.16
    // response.messages.push(SubMsg::new(WasmMsg::Execute {
    //     contract_addr: cfg.reward_contract_addr.to_string(),
    //     funds: vec![],
    //     msg: to_binary(&MirrorExecuteMsg::Unbond {
    //         asset_token: cfg.pair_addr.to_string(),
    //         amount,
    //     })?,
    // }));

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

/// ## Description
/// Handle callbacks described in [`CallbackMsg`]. Returns a [`ContractError`] on failure, otherwise returns a [`Response`]
/// object with the specified submessages if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`CallbackMsg`]. This is the callback action.
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

/// ## Description
/// Transfers LP tokens after withdrawal (from the 3rd party staking contract) to a recipient. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] object with the specified submessages if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **account** is an object of type [`MessageInfo`]. This is the account that receives the LP tokens.
///
/// * **prev_lp_balance** is an object of type [`Uint128`]. This is the previous total amount of LP tokens that were being staked.
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

/// ## Description
/// Exposes all the queries available in the contract.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
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
            let _res: RewardInfoResponse = deps.querier.query_wasm_smart(
                cfg.reward_contract_addr,
                &MirrorQueryMsg::RewardInfo {
                    staker_addr: env.contract.address.to_string(),
                    asset_token: Some(cfg.pair_addr.to_string()),
                },
            )?;
            // TODO:
            // let reward_infos = res.reward_infos;
            // let deposit_amount = if !reward_infos.is_empty() {
            //     reward_infos[0].bond_amount
            // } else {
            //     Uint128::zero()
            // };
            //
            // to_binary(&deposit_amount)
            to_binary(&Uint128::zero())
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
            let _res: RewardInfoResponse = deps.querier.query_wasm_smart(
                cfg.reward_contract_addr,
                &MirrorQueryMsg::RewardInfo {
                    staker_addr: env.contract.address.to_string(),
                    asset_token: Some(cfg.pair_addr.to_string()),
                },
            )?;
            // TODO:
            // let reward_infos = res.reward_infos;
            // let pending_reward = if !reward_infos.is_empty() {
            //     reward_infos[0].pending_reward
            // } else {
            //     Uint128::zero()
            // };
            //
            // to_binary(&pending_reward)
            to_binary(&Uint128::zero())
        }
        QueryMsg::RewardInfo {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&config.reward_token_addr)
        }
    }
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
