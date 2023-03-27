#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, wasm_execute, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg,
    SubMsgResponse, SubMsgResult, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use cw_utils::{must_pay, parse_instantiate_response_data};

use crate::error::ContractError;
use crate::state::CONFIG;
use astroport::native_coin_wrapper::{Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-native-coin-wrapper";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

const TOKEN_SYMBOL_MAX_LENGTH: usize = 8;
const TOKEN_NAME_MAX_LENGTH: usize = 37;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            denom: msg.denom.clone(),
            token: Addr::unchecked(""),
        },
    )?;

    let token_symbol: String = msg.denom.chars().take(TOKEN_SYMBOL_MAX_LENGTH).collect();
    let token_name: String = msg.denom.chars().take(TOKEN_NAME_MAX_LENGTH).collect();

    Ok(Response::new().add_submessage(SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(info.sender.to_string()),
            code_id: msg.token_code_id,
            msg: to_binary(&TokenInstantiateMsg {
                name: format!("CW20-wrapped {}", token_name),
                symbol: token_symbol.to_uppercase(),
                decimals: msg.token_decimals,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            })?,
            funds: vec![],
            label: format!("Astroport {}", token_name),
        }
        .into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: INSTANTIATE_TOKEN_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let mut config = CONFIG.load(deps.storage)?;

            if config.token != Addr::unchecked("") {
                return Err(ContractError::Unauthorized {});
            }

            let init_response = parse_instantiate_response_data(data.as_slice())
                .map_err(|e| StdError::generic_err(format!("{e}")))?;

            config.token = deps.api.addr_validate(&init_response.contract_address)?;

            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new().add_attribute("token_addr", config.token.to_string()))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Wrap {}** Wraps the specified native coin and issues a cw20 token instead.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Wrap {} => wrap(deps, info),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

/// Wraps the specified native coin and issues a cw20 token instead.
pub(crate) fn wrap(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, config.denom.as_str())?;

    let message = wasm_execute(
        config.token.clone(),
        &Cw20ExecuteMsg::Mint {
            recipient: info.sender.to_string(),
            amount,
        },
        vec![],
    )?;

    Ok(Response::new().add_message(message).add_attributes(vec![
        attr("action", "wrap"),
        attr("denom", config.denom),
        attr("token", config.token.to_string()),
        attr("amount", amount.to_string()),
    ]))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
pub(crate) fn receive_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Unwrap {} => {
            // Permission check
            if info.sender != config.token {
                return Err(ContractError::Unauthorized {});
            }

            Ok(Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: config.token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Burn {
                        amount: cw20_msg.amount,
                    })?,
                    funds: vec![],
                }))
                .add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: cw20_msg.sender,
                    amount: vec![Coin {
                        denom: config.denom,
                        amount: cw20_msg.amount,
                    }],
                })))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
    }
}
