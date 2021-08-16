use std::ops::Add;

use cosmwasm_std::{to_binary, Addr, CosmosMsg, DepsMut, Env, Event, MessageInfo, ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg, Deps, Binary};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::Cw20HookMsg;
use terraswap::querier::query_token_balance;

use crate::error::ContractError;
use crate::msg::{ConvertResponse, ExecuteMsg, InitMsg, QueryMsg, QueryAddressResponse};
use crate::querier::{query_pair_info, query_pair_share, query_swap_amount};
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
        ExecuteMsg::ConvertMultiple { token1, token2 } => {
            convert_multiple(&mut deps, env, info, token1, token2)
        }
    }
}

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
        let res = convert(
            deps,
            env.clone(),
            info.clone(),
            token0[i].clone(),
            token1[i].clone(),
        )
        .unwrap();
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
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let mut response = Response::default();

    // get pair lp token
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        state.factory,
        &[token0.clone(), token1.clone()],
    )?;

    // check lp token balance for this contract address
    let balances = query_token_balance(
        &deps.querier,
        pair.liquidity_token.clone(),
        env.contract.address,
    )
    .unwrap();

    // get simulation share for asset balances
    let assets = query_pair_share(&deps.querier, pair.contract_addr.clone(), balances).unwrap();
    let mut amount0 = Uint128::zero();
    let mut amount1 = Uint128::zero();
    for asset in assets {
        if asset.info.equal(&token0.clone()) {
            amount0 = asset.amount;
        }
        if asset.info.equal(&token1.clone()) {
            amount1 = asset.amount;
        }
    }
    // collect tokens from pool(withdraw)
    response.messages.push(SubMsg {
        id: 0,
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&terraswap::pair::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: state.contract.to_string(),
                msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
                amount: balances,
            }))
            .unwrap(),
            funds: vec![],
        }),
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    // swap tokens to astro
    // if !token0.equal(&pair.asset_infos[0]) {
    //     (amount0, amount1) = (amount1, amount0);
    // }
    //example  pool ust-> luna
    // ust->asto +
    // luna-> astro -
    // => luna ->ust -> ust -> astro
    let res = convert_step(deps, token0.clone(), token1.clone(), amount0, amount1).unwrap();
    if let Some(msgs) = res.massages {
        for msg in msgs {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(msg),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }
    if let Some(evts) = res.events {
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
        .attr("astro", res.amount.to_string());
    response.events.push(event);
    Ok(response)
}

fn convert_step(
    deps: &mut DepsMut,
    token0: AssetInfo,
    token1: AssetInfo,
    amount0: Uint128,
    amount1: Uint128,
) -> StdResult<ConvertResponse>
//) -> StdResult<(Uint128, Option<Vec<WasmMsg>>, Option<Vec<Event>>)>
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
            let messages = vec![WasmMsg::Execute {
                contract_addr: state.astro_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: state.staking.to_string(),
                    amount,
                })?,
                funds: vec![],
            }];
            let events = vec![Event::new("TransferToStaking").attr("astroOut", amount.to_string())];
            Ok(ConvertResponse {
                amount,
                massages: Some(messages),
                events: Some(events),
            })
        } else {
            let mut messages = Vec::new();
            let mut events = Vec::new();
            let res = to_astro(deps, token0, amount).unwrap();
            let amount = res.amount;
            if let Some(msgs) = res.massages {
                for msg in msgs {
                    messages.push(msg);
                }
            }
            if let Some(evts) = res.events {
                for evt in evts {
                    events.push(evt);
                }
            }
            let res = convert_step(deps, astro.clone(), astro, amount, Uint128::zero()).unwrap();
            if let Some(msgs) = res.massages {
                for msg in msgs {
                    messages.push(msg);
                }
            }
            if let Some(evts) = res.events {
                for evt in evts {
                    events.push(evt);
                }
            }
            Ok(ConvertResponse {
                amount: amount.add(res.amount),
                massages: Some(messages),
                events: Some(events),
            })
        }
    } else if token0.equal(&astro) {
        let mut messages = vec![WasmMsg::Execute {
            contract_addr: state.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: state.staking.to_string(),
                amount: amount0,
            })?,
            funds: vec![],
        }];
        let mut events =
            vec![Event::new("TransferToStaking").attr("astroOut", amount0.to_string())];
        let res = to_astro(deps, token1, amount1).unwrap();
        if let Some(msgs) = res.massages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = res.events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok(ConvertResponse {
            amount: amount0.add(res.amount),
            massages: Some(messages),
            events: Some(events),
        })
    } else if token1.equal(&astro) {
        let mut messages = vec![WasmMsg::Execute {
            contract_addr: state.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: state.staking.to_string(),
                amount: amount1,
            })?,
            funds: vec![],
        }];
        let mut events =
            vec![Event::new("TransferToStaking").attr("astroOut", amount1.to_string())];

        let res = to_astro(deps, token0, amount0).unwrap();
        if let Some(msgs) = res.massages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = res.events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok(ConvertResponse {
            amount: amount0.add(res.amount),
            massages: Some(messages),
            events: Some(events),
        })
    } else {
        // eg. MIC - USDT
        let mut messages = vec![];
        let mut events = vec![];
        let res = swap(deps, token0, astro.clone(), amount0, state.contract.clone()).unwrap();
        let amount0 = res.amount;
        if let Some(msgs) = res.massages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = res.events {
            for evt in evts {
                events.push(evt);
            }
        }
        let res = swap(deps, token1, astro.clone(), amount1, state.contract).unwrap();
        let amount1 = res.amount;
        if let Some(msgs) = res.massages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = res.events {
            for evt in evts {
                events.push(evt);
            }
        }
        let res = convert_step(deps, astro.clone(), astro, amount0, amount1).unwrap();
        if let Some(msgs) = res.massages {
            for msg in msgs {
                messages.push(msg);
            }
        }
        if let Some(evts) = res.events {
            for evt in evts {
                events.push(evt);
            }
        }
        Ok(ConvertResponse {
            amount: res.amount,
            massages: Some(messages),
            events: Some(events),
        })
    }
}

fn swap(
    deps: &mut DepsMut,
    from_token: AssetInfo,
    to_token: AssetInfo,
    amount_in: Uint128,
    to: Addr,
) -> StdResult<ConvertResponse> {
    let state = STATE.load(deps.storage)?;
    // Checks
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        state.factory,
        &[from_token.clone(), to_token],
    )?;

    // Interactions
    // let mut reserve0 = Uint128::zero();
    // let mut reserve1 = Uint128::zero();
    // let assets = query_pair_share(&deps.querier, pair.contract_addr.clone(), amount_in).unwrap();
    // for asset in assets{
    //     if asset.info.equal(&from_token.clone()) {
    //         reserve0 = asset.amount;
    //     }
    //     if asset.info.equal(&to_token.clone()) {
    //         reserve1 = asset.amount;
    //     }
    // }
    // let amount_in_with_fee = amount_in.checked_mul(Uint128::from(997u128)).unwrap();
    // let amount_out = amount_in_with_fee
    //     .checked_mul(reserve1)
    //     .unwrap()
    //     .checked_div(
    //         reserve0
    //             .checked_mul(Uint128::from(1000u128))
    //             .unwrap()
    //             .checked_add(amount_in_with_fee)
    //             .unwrap()
    //     )
    //     .unwrap();
    let amount_out = query_swap_amount(
        &deps.querier,
        pair.contract_addr.clone(),
        from_token.clone(),
        amount_in,
    )
    .unwrap();
    let messages = vec![
        WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: pair.contract_addr.to_string(),
                amount: amount_in,
            })?,
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: Asset {
                    info: from_token,
                    amount: amount_out,
                },
                belief_price: None,
                max_spread: None,
                to: Option::from(to.to_string()),
            })?,
            funds: vec![],
        },
    ];
    let events = vec![Event::new("Swap").attr("AmountOut", amount_out.to_string())];
    Ok(ConvertResponse {
        amount: amount_out,
        massages: Some(messages),
        events: Some(events),
    })
}

fn to_astro(
    deps: &mut DepsMut,
    token: AssetInfo,
    amount_in: Uint128,
) -> StdResult<ConvertResponse> {
    let state = STATE.load(deps.storage)?;
    swap(
        deps,
        token,
        AssetInfo::Token {
            contract_addr: state.astro_token,
        },
        amount_in,
        state.staking,
    )
}

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetFactory {} => to_binary(&query_get_factory(deps)?),
    }
}

fn query_get_factory( deps: Deps) -> StdResult<QueryAddressResponse>{
    let config = STATE.load(deps.storage)?;
    Ok( QueryAddressResponse{address:config.factory})
}


