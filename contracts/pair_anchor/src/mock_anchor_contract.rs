use astroport::pair_anchor::{StateResponse, AnchorQueryMsg, AnchorExecuteMsg};

use cosmwasm_std::{Addr};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point,
    attr, from_binary, to_binary, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, Reply, Response, StdResult, Uint128, WasmMsg,
};
use cw20::{Cw20ReceiveMsg, Cw20ExecuteMsg};

use std::str::FromStr;

use crate::error::ContractError;


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AnchorInstantiateMsg {
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
struct Config {
    aterra_contract: CanonicalAddr
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
        AnchorExecuteMsg::RedeemStable { .. } => Err(ContractError::NonSupported {}),
        AnchorExecuteMsg::SetToken(aterra_contract) => {
            
            let config = Config {
                aterra_contract: deps.api.addr_canonicalize(aterra_contract.as_str())? 
            };

            CONFIG.save(deps.storage, &config)?;
            Ok(Response::new())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response, ContractError> {    
    Ok(Response::new())
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg) {
        Ok(AnchorExecuteMsg::RedeemStable {}) => {
            let cw20_sender_addr = deps.api.addr_validate(&cw20_msg.sender)?;
            redeem_stable(deps, env, cw20_sender_addr, cw20_msg.amount)
        }
        _ => Err(ContractError::NonSupported { }),
    }
}


pub fn deposit_stable(
    deps: Deps,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {

    // Check base denom deposit
    let deposit_amount: Uint256 = info
        .funds
        .iter()
        .find(|c| c.denom == *"uusd")
        .map(|c| Uint256::from(c.amount))
        .unwrap_or_else(Uint256::zero);

    // Load anchor token exchange rate with updated state
    let exchange_rate = compute_exchange_rate(deps, env);
    let mint_amount = deposit_amount / exchange_rate;
    
    println!("Anchor->Exchange Rate {:?}", exchange_rate.to_string());
    println!("Anchor->Mint amount {:?}", mint_amount);
    println!("Anchor->Recipient {:?}", info.sender.to_string());

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
    
    
    println!("{:?}", config);
    println!("{:?}", exchange_rate);
    println!("{:?}", redeem_amount);

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
                amount: vec![
                    Coin {
                        denom: "uusd".to_string(),
                        amount: redeem_amount.into(),
                    },
                ],
            }),
        ])
        .add_attributes(vec![
            attr("action", "redeem_stable"),
            attr("burn_amount", burn_amount),
            attr("redeem_amount", redeem_amount),
        ]))
}

fn compute_exchange_rate(deps: Deps, env: Env) -> Decimal256 {
    query_state(deps, env, None).unwrap().prev_exchange_rate
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: AnchorQueryMsg) -> StdResult<Binary> {
    match msg {
        AnchorQueryMsg::State { block_height } => to_binary(&query_state(deps, env, block_height)?),        
    }
}


pub fn query_state(_deps: Deps, _env: Env, _block_height: Option<u64>) -> StdResult<StateResponse> {    
    Ok(StateResponse {
        total_liabilities: Decimal256::from_str(
            "2771617807710230.916899829317330205",
        )
        .unwrap(),
        total_reserves: Decimal256::from_str("0.734860156808943602")
            .unwrap(),
        last_interest_updated: 6944488u64,
        last_reward_updated: 6944488u64,
        global_interest_index: Decimal256::from_str("1.239677817386941132")
            .unwrap(),
        global_reward_index: Decimal256::from_str("0.257774735210970248")
            .unwrap(),
        anc_emission_rate: Decimal256::from_str(
            "20381363.85157231012364762",
        )
        .unwrap(),
        prev_aterra_supply: Uint256::from_str("9253988242307733").unwrap(),
        prev_exchange_rate: Decimal256::from_str("1.216736524026807943")
            .unwrap(),
    })
}