use cosmwasm_bignumber::{Decimal256, Uint256};
use cw_storage_plus::Item;
use moneymarket::market::{
    Cw20HookMsg, EpochStateResponse, ExecuteMsg as AnchorExecuteMsg, QueryMsg as AnchorQueryMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, BankMsg, Binary, CanonicalAddr, Coin,
    CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport_pair_anchor::error::ContractError;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AnchorInstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
struct Config {
    aterra_contract: CanonicalAddr,
}

const CONFIG: Item<Config> = Item::new("config");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: AnchorInstantiateMsg,
) -> Result<Response, ContractError> {
    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: AnchorExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        AnchorExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        AnchorExecuteMsg::DepositStable {} => deposit_stable(deps.as_ref(), env, info),
        AnchorExecuteMsg::RegisterContracts {
            overseer_contract, ..
        } => {
            // using this execute message to set the correct state for mocking it
            let config = Config {
                aterra_contract: deps.api.addr_canonicalize(overseer_contract.as_str())?,
            };

            CONFIG.save(deps.storage, &config)?;
            Ok(Response::new())
        }

        _ => panic!("Do not enter here"),
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::RedeemStable {}) => {
            let cw20_sender_addr = deps.api.addr_validate(&cw20_msg.sender)?;
            redeem_stable(deps, env, cw20_sender_addr, cw20_msg.amount)
        }
        _ => Err(ContractError::NonSupported {}),
    }
}

pub fn deposit_stable(deps: Deps, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // Check base denom deposit
    let deposit_amount: Uint256 = info
        .funds
        .iter()
        .find(|c| c.denom.as_str() == "uusd")
        .map(|c| Uint256::from(c.amount))
        .unwrap_or_else(Uint256::zero);

    // Load anchor token exchange rate with updated state
    let exchange_rate = compute_exchange_rate(deps, env);
    let mint_amount = deposit_amount / exchange_rate;

    let config: Config = CONFIG.load(deps.storage)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.aterra_contract)?.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: info.sender.to_string(),
                amount: mint_amount.into(),
            })?,
        }))
        .add_attributes(vec![
            attr("action", "deposit_stable"),
            attr("depositor", info.sender),
            attr("mint_amount", mint_amount),
            attr("deposit_amount", deposit_amount),
        ]))
}

pub fn redeem_stable(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    burn_amount: Uint128,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let exchange_rate = compute_exchange_rate(deps.as_ref(), env);
    let redeem_amount = Uint256::from(burn_amount) * exchange_rate;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.aterra_contract)?.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Burn {
                    amount: burn_amount,
                })?,
            }),
            CosmosMsg::Bank(BankMsg::Send {
                to_address: sender.to_string(),
                amount: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: redeem_amount.into(),
                }],
            }),
        ])
        .add_attributes(vec![
            attr("action", "redeem_stable"),
            attr("burn_amount", burn_amount),
            attr("redeem_amount", redeem_amount),
        ]))
}

fn compute_exchange_rate(deps: Deps, env: Env) -> Decimal256 {
    query_state(deps, env).unwrap().exchange_rate
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: AnchorQueryMsg) -> StdResult<Binary> {
    match msg {
        AnchorQueryMsg::EpochState { .. } => to_binary(&query_state(deps, env)?),
        _ => panic!("Do not enter here"),
    }
}

pub fn query_state(_deps: Deps, _env: Env) -> StdResult<EpochStateResponse> {
    Ok(EpochStateResponse {
        aterra_supply: Uint256::from_str("9253988242307733").unwrap(),
        exchange_rate: Decimal256::from_str("1.216736524026807943").unwrap(),
    })
}
