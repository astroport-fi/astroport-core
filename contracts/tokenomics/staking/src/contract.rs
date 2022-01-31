use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::staking::{ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};
use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, MinterResponse,
    TokenInfoResponse,
};

use crate::response::MsgInstantiateContractResponse;
use astroport::asset::addr_validate_to_lower;
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use protobuf::Message;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-staking";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Staking information.
const TOKEN_NAME: &str = "astroport-staking-token";
const TOKEN_SYMBOL: &str = "xASTRO";

/// A `reply` call code ID of sub-message.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful,
/// or the [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for
/// creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Store config
    CONFIG.save(
        deps.storage,
        &Config {
            astro_token_addr: addr_validate_to_lower(deps.api, &msg.deposit_token_addr)?,
            xastro_token_addr: Addr::unchecked(""),
        },
    )?;

    // Create token
    let sub_msg: Vec<SubMsg> = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(msg.owner),
            code_id: msg.token_code_id,
            msg: to_binary(&TokenInstantiateMsg {
                name: TOKEN_NAME.to_string(),
                symbol: TOKEN_SYMBOL.to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
            })?,
            funds: vec![],
            label: String::from("Astroport Staking Token"),
        }
        .into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new().add_submessages(sub_msg))
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
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
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

/// # Description
/// The entry point to the contract for processing the reply from the submessage.
/// Sets the xASTRO contract address.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`Reply`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.xastro_token_addr != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    // Set token addr
    config.xastro_token_addr = addr_validate_to_lower(deps.api, res.get_contract_address())?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    let recipient = cw20_msg.sender;
    let amount = cw20_msg.amount;

    let mut total_deposit = get_total_deposit(deps.as_ref(), env, config.clone())?;
    let total_shares = get_total_shares(deps.as_ref(), config.clone())?;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Enter {} => {
            if info.sender != config.astro_token_addr {
                return Err(ContractError::Unauthorized {});
            }
            // In cw20 send total balance is already increased,
            // To calculated properly we should subtract user deposit from the pool
            total_deposit -= amount;
            let mint_amount: Uint128 = if total_shares.is_zero() || total_deposit.is_zero() {
                amount
            } else {
                amount
                    .checked_mul(total_shares)?
                    .checked_div(total_deposit)
                    .map_err(|e| StdError::DivideByZero { source: e })?
            };

            let res = Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.xastro_token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient,
                    amount: mint_amount,
                })?,
                funds: vec![],
            }));

            Ok(res)
        }
        Cw20HookMsg::Leave {} => {
            if info.sender != config.xastro_token_addr {
                return Err(ContractError::Unauthorized {});
            }

            let what = amount
                .checked_mul(total_deposit)?
                .checked_div(total_shares)
                .map_err(|e| StdError::DivideByZero { source: e })?;

            // Burn share
            let res = Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: config.xastro_token_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
                    funds: vec![],
                }))
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: config.astro_token_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient,
                        amount: what,
                    })?,
                    funds: vec![],
                }));

            Ok(res)
        }
    }
}

/// ## Description
/// Returns the total shares of staked xASTRO tokens.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **config** is the object of type [`Config`].
pub fn get_total_shares(deps: Deps, config: Config) -> StdResult<Uint128> {
    let result: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(&config.xastro_token_addr, &Cw20QueryMsg::TokenInfo {})?;

    Ok(result.total_supply)
}

/// ## Description
/// Returns the total deposit of staked ASTRO tokens.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **config** is the object of type [`Config`].
pub fn get_total_deposit(deps: Deps, env: Env, config: Config) -> StdResult<Uint128> {
    let result: BalanceResponse = deps.querier.query_wasm_smart(
        &config.astro_token_addr,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    Ok(result.balance)
}

/// # Description
/// Describes all query messages.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns information about the staking configs
/// in a [`ConfigResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&ConfigResponse {
            deposit_token_addr: config.astro_token_addr,
            share_token_addr: config.xastro_token_addr,
        })?),
    }
}
