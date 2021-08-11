use std::ops::Add;

use cosmwasm_std::{Addr, CosmosMsg, DepsMut, Env, Event, MessageInfo, ReplyOn, Response, StdResult, SubMsg, to_binary, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::Cw20HookMsg;
use terraswap::querier::query_token_balance;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InitMsg};
use crate::querier::query_pair_info;
use crate::state::{State, STATE};

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InitMsg,
) -> Result<Response, ContractError> {
    let state = State {
        owner: info.sender,
        contract: env.contract.address,
        factory: msg.factory,
        staking: msg.staking,
        astro_token: msg.astro,
    };
    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}


pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        //ExecuteMsg::SetBridge { token, bridge } => set_bridge(deps, env, info, token, bridge),
        ExecuteMsg::Convert { token1, token2 } => try_convert(&mut deps, env, info, token1, token2),
        ExecuteMsg::ConvertMultiple { token1, token2 } => convert_multiple(&mut deps, env, info, token1, token2),
    }
}

// pub fn set_bridge(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
//     token: Addr,
//     bridge: Addr,
// ) -> Result<Response, ContractError> {
//     let state = STATE.load(deps.storage).unwrap();
//     let mut response = Response::default();
//
//     if info.sender != state.owner {
//         return Err(ContractError::Unauthorized {});
//     }
//     if token == state.astro_token || token == bridge {
//         return Err(ContractError::InvalidBridge {});
//     }
//     BRIDGES.save(deps.storage, &token, &bridge)?;
//     let event = Event::new("SetBridge")
//         .attr("Token", token.to_string())
//         .attr("Bridge", bridge.to_string());
//     response.add_event(event);
//     Ok(response)
// }

pub fn try_convert(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: AssetInfo,
    token1: AssetInfo,
) -> Result<Response, ContractError> {
    convert(deps, env, info, token0, token1)
}


pub fn convert_multiple(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: Vec<AssetInfo>,
    token1: Vec<AssetInfo>,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    let len = token0.len();
    for i in 0..len {
        let res = convert(deps, env.clone(), info.clone(), token0[i].clone(), token1[i].clone()).unwrap();
        for msg in res.messages {
            response.messages.push(msg);
        }
        for event in res.events {
            response.events.push(event);
        }
    }
    Ok(response)
}

fn convert(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: AssetInfo,
    token1: AssetInfo,
) -> Result<Response, ContractError>
{
    let state = STATE.load(deps.storage)?;
    let mut response = Response::default();

    // get pair lp token
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        state.factory,
        &[token0.clone(), token1.clone()],
    )?;

    // check lp token balance for this contract address
    let balances = query_token_balance(&deps.querier, pair.liquidity_token, env.contract.address).unwrap();

    // get simulation share for asset balances
    // X1 - X5: OK
    // (uint256 amount0, uint256 amount1) = pair.burn(address(this));
    let amount0 = Uint128::zero();
    let amount1 = Uint128::zero();

    // balanceOf: S1 - S4: OK
    // transfer: X1 - X5: OK
    // collect tokens from pool(withdraw)
    response.messages.push(
        SubMsg {
            id: 0,
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair.contract_addr.to_string(),
                msg: to_binary(&terraswap::pair::ExecuteMsg::Receive(
                    Cw20ReceiveMsg {
                        sender: state.contract.to_string(),
                        msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
                        amount: balances,
                    }
                )).unwrap(),
                funds: vec![],
            }),
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    // swap tokens to astro
    // if !token0.equal(&pair.asset_infos[0]) {
    //     (amount0, amount1) = (amount1, amount0);
    // }
    //example  pool ust-> luna
    // ust->asto +
    // luna-> astro -
    // => luna ->ust -> ust -> astro
    let (asrto_out, messages, events) = convert_step(deps, token0.clone(), token1.clone(), amount0, amount1).unwrap();
    if let Some(msgs) = messages {
        for msg in msgs {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(msg),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }
    if let Some(evts) = events {
        for evt in evts {
            response.events.push(evt);
        }
    }
    let event = Event::new("LogConvert")
        .attr("sender", info.sender.to_string())
        .attr("token0", token0.to_string())
        .attr("token1", token1.to_string())
        .attr("amount0", amount0.to_string())
        .attr("amount1", amount1.to_string())
        .attr("astro", asrto_out.to_string());
    response.events.push(event);
    Ok(response)
}

fn convert_step(deps: &mut DepsMut, token0: AssetInfo, token1: AssetInfo, amount0: Uint128, amount1: Uint128) -> StdResult<(Uint128, Option<Vec<WasmMsg>>, Option<Vec<Event>>)>
{
    let state = STATE.load(deps.storage)?;
    let astro = AssetInfo::Token {
        contract_addr: state.astro_token.clone(),
    };
    // Interactions
    if token0.equal(&token1) {
        let amount = amount0.add(amount1);
        if token0.equal(&astro) {
            // transfer all astro to bar
            let messages = vec![
                WasmMsg::Execute {
                    contract_addr: state.astro_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: state.staking.to_string(),
                        amount,
                    })?,
                    funds: vec![],
                }
            ];
            let events = vec![
                Event::new("TransferToStaking")
                    .attr("astroOut", amount.to_string())
            ];
            Ok((amount, Some(messages), Some(events)))
        } else {
            let mut messages = Vec::new();
            let mut events = Vec::new();
            let (swap_astro, swap_messages, swap_events) = to_astro(deps, token0, amount).unwrap();
            if let Some(msgs) = swap_messages {
                for msg in msgs {
                    messages.push(msg);
                }
            }
            if let Some(evts) = swap_events {
                for evt in evts {
                    events.push(evt);
                }
            }
            let (astro, convert_msg, convert_events) = convert_step(deps, astro.clone(), astro, amount, Uint128::zero()).unwrap();
            if let Some(msgs) = convert_msg {
                for msg in msgs {
                    messages.push(msg);
                }
            }
            if let Some(evts) = convert_events {
                for evt in evts {
                    events.push(evt);
                }
            }
            Ok((swap_astro.add(astro), Some(messages), Some(events)))
        }
    } else if token0.equal(&astro) {
        let mut messages = vec![
            WasmMsg::Execute {
                contract_addr: state.astro_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: state.staking.to_string(),
                    amount: amount0,
                })?,
                funds: vec![],
            }
        ];
        let mut events = vec![
            Event::new("TransferToStaking")
                .attr("astroOut", amount0.to_string())
        ];
        let (astro, swap_msgs, swap_events) = to_astro(deps, token1, amount1).unwrap();
        if let Some(msgs) = swap_msgs {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = swap_events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok((amount0.add(astro), Some(messages), Some(events)))
    } else if token1.equal(&astro) {
        let mut messages = vec![
            WasmMsg::Execute {
                contract_addr: state.astro_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: state.staking.to_string(),
                    amount: amount1,
                })?,
                funds: vec![],
            }
        ];
        let mut events = vec![
            Event::new("TransferToStaking")
                .attr("astroOut", amount1.to_string())
        ];

        let (astro, swap_msgs, swap_events) = to_astro(deps, token0, amount0).unwrap();
        if let Some(msgs) = swap_msgs {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = swap_events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok((amount0.add(astro), Some(messages), Some(events)))
    } else {
        // eg. MIC - USDT
        let mut messages = vec![];
        let mut events = vec![];
        let (astro0, messages0, events0) = swap(deps, token0, astro.clone(), amount0, state.contract.clone()).unwrap();
        if let Some(msgs) = messages0 {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = events0 {
            for evt in evts {
                events.push(evt);
            }
        }
        let (astro1, messages1, events1) = swap(deps, token1, astro.clone(), amount1, state.contract).unwrap();
        if let Some(msgs) = messages1 {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = events1 {
            for evt in evts {
                events.push(evt);
            }
        }
        let (astro, convert_messages, convert_events) = convert_step(deps, astro.clone(), astro, amount0, amount1).unwrap();
        if let Some(msgs) = convert_messages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = convert_events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok((astro0.add(astro1).add(astro), Some(messages), Some(events)))
    }
}

fn swap(deps: &mut DepsMut, from_token: AssetInfo, to_token: AssetInfo, amount_in: Uint128, to: Addr) -> StdResult<(Uint128, Option<Vec<WasmMsg>>, Option<Vec<Event>>)> {
    let state = STATE.load(deps.storage)?;
    // Checks
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        state.factory,
        &[from_token.clone(), to_token],
    )?;

    // Interactions
    let reserve0 = Uint128::zero();
    let reserve1 = Uint128::zero();

    let amount_in_with_fee = amount_in.checked_mul(Uint128::from(997u128)).unwrap();
    let amount_out = amount_in_with_fee
        .checked_mul(reserve1)
        .unwrap()
        .checked_div(
            reserve0
                .checked_mul(Uint128::from(1000u128))
                .unwrap()
                .checked_add(amount_in_with_fee)
                .unwrap()
        )
        .unwrap();
    let messages = vec![
        WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: state.contract.to_string(),
                amount: amount_in,
            })?,
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: Asset { info: from_token, amount: amount_in },
                belief_price: None,
                max_spread: None,
                to: Option::from(to.to_string()),
            })?,
            funds: vec![],
        },
    ];
    let events = vec![
        Event::new("Swap")
            .attr("AmountOut", amount_out.to_string()),
    ];
    Ok((reserve0.add(reserve1), Some(messages), Some(events)))
}


fn to_astro(deps: &mut DepsMut, token: AssetInfo, amount_in: Uint128) -> StdResult<(Uint128, Option<Vec<WasmMsg>>, Option<Vec<Event>>)> {
    let state = STATE.load(deps.storage)?;
    swap(deps, token, AssetInfo::Token { contract_addr: state.astro_token }, amount_in, state.staking)
}
