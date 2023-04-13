use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, wasm_execute, Addr, Binary, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};

use crate::response::MsgInstantiateContractResponse;
use astroport::querier::{query_supply, query_token_balance};
use astroport::xastro_token::InstantiateMsg as TokenInstantiateMsg;
use protobuf::Message;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-staking";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// xASTRO information.
const TOKEN_NAME: &str = "Staked Astroport";
const TOKEN_SYMBOL: &str = "xASTRO";

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

/// Minimum initial xastro share
pub(crate) const MINIMUM_STAKE_AMOUNT: Uint128 = Uint128::new(1_000);

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
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
            astro_token_addr: deps.api.addr_validate(&msg.deposit_token_addr)?,
            xastro_token_addr: Addr::unchecked(""),
        },
    )?;

    // Create the xASTRO token
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
                marketing: msg.marketing,
            })?,
            funds: vec![],
            label: String::from("Staked Astroport Token"),
        }
        .into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new().add_submessages(sub_msg))
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
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

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_TOKEN_REPLY_ID => {
            let mut config = CONFIG.load(deps.storage)?;

            if config.xastro_token_addr != Addr::unchecked("") {
                return Err(ContractError::Unauthorized {});
            }

            let data = msg.result.unwrap().data.unwrap();
            let res: MsgInstantiateContractResponse = Message::parse_from_bytes(data.as_slice())
                .map_err(|_| {
                    StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
                })?;

            config.xastro_token_addr = deps.api.addr_validate(res.get_contract_address())?;

            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new())
        }
        _ => Err(StdError::generic_err(format!("Unknown reply ID: {}", msg.id)).into()),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    let recipient = cw20_msg.sender;
    let mut amount = cw20_msg.amount;

    let mut total_deposit = query_token_balance(
        &deps.querier,
        &config.astro_token_addr,
        env.contract.address.clone(),
    )?;
    let total_shares = query_supply(&deps.querier, &config.xastro_token_addr)?;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Enter {} => {
            let mut messages = vec![];
            if info.sender != config.astro_token_addr {
                return Err(ContractError::Unauthorized {});
            }

            // In a CW20 `send`, the total balance of the recipient is already increased.
            // To properly calculate the total amount of ASTRO deposited in staking, we should subtract the user deposit from the pool
            total_deposit -= amount;
            let mint_amount: Uint128 = if total_shares.is_zero() || total_deposit.is_zero() {
                amount = amount
                    .checked_sub(MINIMUM_STAKE_AMOUNT)
                    .map_err(|_| ContractError::MinimumStakeAmountError {})?;

                // amount cannot become zero after minimum stake subtraction
                if amount.is_zero() {
                    return Err(ContractError::MinimumStakeAmountError {});
                }

                messages.push(wasm_execute(
                    config.xastro_token_addr.clone(),
                    &Cw20ExecuteMsg::Mint {
                        recipient: env.contract.address.to_string(),
                        amount: MINIMUM_STAKE_AMOUNT,
                    },
                    vec![],
                )?);

                amount
            } else {
                amount = amount
                    .checked_mul(total_shares)?
                    .checked_div(total_deposit)?;

                if amount.is_zero() {
                    return Err(ContractError::StakeAmountTooSmall {});
                }

                amount
            };

            messages.push(wasm_execute(
                config.xastro_token_addr,
                &Cw20ExecuteMsg::Mint {
                    recipient: recipient.clone(),
                    amount: mint_amount,
                },
                vec![],
            )?);

            Ok(Response::new().add_messages(messages).add_attributes(vec![
                attr("action", "enter"),
                attr("recipient", recipient),
                attr("astro_amount", cw20_msg.amount),
                attr("xastro_amount", mint_amount),
            ]))
        }
        Cw20HookMsg::Leave {} => {
            if info.sender != config.xastro_token_addr {
                return Err(ContractError::Unauthorized {});
            }

            let what = amount
                .checked_mul(total_deposit)?
                .checked_div(total_shares)?;

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
                        recipient: recipient.clone(),
                        amount: what,
                    })?,
                    funds: vec![],
                }));

            Ok(res.add_attributes(vec![
                attr("action", "leave"),
                attr("recipient", recipient),
                attr("xastro_amount", cw20_msg.amount),
                attr("astro_amount", what),
            ]))
        }
    }
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the staking contract configuration using a [`ConfigResponse`] object.
///
/// * **QueryMsg::TotalShares {}** Returns the total xASTRO supply using a [`Uint128`] object.
///
/// * **QueryMsg::Config {}** Returns the amount of ASTRO that's currently in the staking pool using a [`Uint128`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&ConfigResponse {
            deposit_token_addr: config.astro_token_addr,
            share_token_addr: config.xastro_token_addr,
        })?),
        QueryMsg::TotalShares {} => {
            to_binary(&query_supply(&deps.querier, &config.xastro_token_addr)?)
        }
        QueryMsg::TotalDeposit {} => to_binary(&query_token_balance(
            &deps.querier,
            &config.astro_token_addr,
            env.contract.address,
        )?),
    }
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-staking" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" | "1.0.2" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
