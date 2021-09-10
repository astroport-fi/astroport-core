use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo,
    Reply, ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::msg::{ConvertStepResponse, ExecuteMsg, InstantiateMsg, QueryAddressResponse, QueryMsg};
use crate::querier::{query_pair_info, query_pair_share, query_swap_amount};
use crate::state::{Config, ExecuteOnReply, CONFIG, CONVERT_MULTIPLE};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::Cw20HookMsg;
use astroport::querier::query_token_balance;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let cfg = Config {
        owner: info.sender,
        factory_contract: deps.api.addr_validate(&msg.factory_contract)?,
        staking_contract: deps.api.addr_validate(&msg.staking_contract)?,
        astro_token_contract: deps.api.addr_validate(&msg.astro_token_contract)?,
    };
    CONFIG.save(deps.storage, &cfg)?;
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
        ExecuteMsg::Convert { token1, token2 } => convert(deps, env, info.sender, token1, token2),
        ExecuteMsg::ConvertMultiple { token1, token2 } => {
            convert_multiple(deps, env, info.sender, token1, token2)
        }
    }
}

pub fn convert_multiple(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    token0: Vec<AssetInfo>,
    token1: Vec<AssetInfo>,
) -> Result<Response, ContractError> {
    if sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    let mut tokens = CONVERT_MULTIPLE.load(deps.storage).unwrap_or_default();
    if tokens.token0.len() == 1 {
        let t0 = tokens.token0.swap_remove(0);
        let t1 = tokens.token1.swap_remove(0);
        CONVERT_MULTIPLE.save(deps.storage, &ExecuteOnReply { token0, token1 })?;
        return convert(deps, env, sender, t0, t1);
    }
    Ok(Response::new().add_submessage(SubMsg::reply_on_success(
        convert_and_execute(deps, env, token0, token1)?,
        0,
    )))
}

fn convert_and_execute(
    deps: DepsMut,
    env: Env,
    mut token0: Vec<AssetInfo>,
    mut token1: Vec<AssetInfo>,
) -> StdResult<CosmosMsg> {
    let t0 = token0.swap_remove(0);
    let t1 = token1.swap_remove(0);
    CONVERT_MULTIPLE.save(deps.storage, &ExecuteOnReply { token0, token1 })?;
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::Convert {
            token1: t0,
            token2: t1,
        })?,
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    let tokens = CONVERT_MULTIPLE.load(deps.storage)?;
    convert_multiple(
        deps,
        env.clone(),
        env.contract.address,
        tokens.token0,
        tokens.token1,
    )
}

fn convert(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    token0: AssetInfo,
    token1: AssetInfo,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let mut response = Response::default();

    // get pair lp token
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract.to_string(),
        &[token0.clone(), token1.clone()],
    )?;

    // check lp token balance for this contract address
    let balances = query_token_balance(
        &deps.querier,
        pair.liquidity_token.clone(),
        env.contract.address.clone(),
    )
    .unwrap();

    // get simulation share for asset balances
    let assets = query_pair_share(&deps.querier, pair.contract_addr.clone(), balances).unwrap();
    let (amount0, amount1) = (
        assets
            .iter()
            .find(|a| a.info.equal(&token0))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
        assets
            .iter()
            .find(|a| a.info.equal(&token1))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
    );

    // collect tokens from pool(withdraw)
    let funds = if token1.is_native_token() {
        vec![Coin {
            denom: token1.to_string(),
            amount: amount1,
        }]
    } else if token0.is_native_token() {
        vec![Coin {
            denom: token0.to_string(),
            amount: amount0,
        }]
    } else {
        vec![]
    };

    response.messages.push(SubMsg {
        id: 0,
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair.liquidity_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair.contract_addr.to_string(),
                amount: balances,
                msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
            })
            .unwrap(),
            funds,
        }),
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    // swap tokens to astro
    let res = convert_step(
        deps,
        env,
        cfg,
        token0.clone(),
        token1.clone(),
        amount0,
        amount1,
    )
    .unwrap();
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
        .add_attribute("sender", sender.to_string())
        .add_attribute("token0", token0.to_string())
        .add_attribute("token1", token1.to_string())
        .add_attribute("amount0", amount0.to_string())
        .add_attribute("amount1", amount1.to_string())
        .add_attribute("astro", res.amount.to_string());
    response.events.push(event);
    Ok(response)
}

fn convert_step(
    mut deps: DepsMut,
    env: Env,
    cfg: Config,
    token0: AssetInfo,
    token1: AssetInfo,
    amount0: Uint128,
    amount1: Uint128,
) -> StdResult<ConvertStepResponse> {
    let astro = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };
    // Interactions
    if token0.equal(&token1) {
        let amount = amount0.checked_add(amount1)?;
        if token0.equal(&astro) {
            transfer_astro(&cfg, amount)
        } else {
            let convert_step_response = swap_to_astro(deps.branch(), cfg.clone(), token0, amount).unwrap();
            let res = convert_step(
                deps,
                env,
                cfg,
                astro.clone(),
                astro,
                amount,
                Uint128::zero(),
            )
            .unwrap();
            Ok(convert_step_response.push_msg(res))
        }
    } else if token0.equal(&astro) {
        let convert_step_response = transfer_astro(&cfg, amount0)?;
        let res = swap_to_astro(deps, cfg, token1, amount1).unwrap();
        Ok(convert_step_response.push_msg(res))
    } else if token1.equal(&astro) {
        let convert_step_response = transfer_astro(&cfg, amount1)?;
        let res = swap_to_astro(deps, cfg, token0, amount0).unwrap();
        Ok(convert_step_response.push_msg(res))
    } else {
        // eg. MIC - USDT
        let convert_step_resp = swap(
            deps.branch(),
            cfg.clone(),
            token0,
            astro.clone(),
            amount0,
            env.contract.address.clone(),
        )
        .unwrap();
        let res = swap(
            deps.branch(),
            cfg.clone(),
            token1,
            astro.clone(),
            amount1,
            env.contract.address.clone(),
        )
        .unwrap();
        let convert_step_response = convert_step_resp.push_msg(res);
        let res = convert_step(deps, env, cfg, astro.clone(), astro, amount0, amount1).unwrap();
        Ok(convert_step_response.push_msg(res))
    }
}

fn transfer_astro(cfg: &Config, amount: Uint128) -> StdResult<ConvertStepResponse> {
    let messages = vec![WasmMsg::Execute {
        contract_addr: cfg.astro_token_contract.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: cfg.staking_contract.to_string(),
            amount,
        })?,
        funds: vec![],
    }];
    let events =
        vec![Event::new("TransferToStaking").add_attribute("astroOut", amount.to_string())];
    Ok(ConvertStepResponse {
        amount,
        massages: Some(messages),
        events: Some(events),
    })
}

fn swap(
    deps: DepsMut,
    cfg: Config,
    from_token: AssetInfo,
    to_token: AssetInfo,
    amount_in: Uint128,
    to: Addr,
) -> StdResult<ConvertStepResponse> {
    // Checks
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract.to_string(),
        &[from_token.clone(), to_token],
    )?;
    // Interactions
    let amount_out = query_swap_amount(
        &deps.querier,
        pair.contract_addr.clone(),
        from_token.clone(),
        amount_in,
    )
    .unwrap();

    let messages = if from_token.is_native_token() {
        vec![WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset: Asset {
                    info: from_token.clone(),
                    amount: amount_out,
                },
                belief_price: None,
                max_spread: None,
                to: Option::from(to.to_string()),
            })?,
            funds: vec![Coin {
                denom: from_token.to_string(),
                amount: amount_out,
            }],
        }]
    } else {
        vec![WasmMsg::Execute {
            contract_addr: from_token.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pair.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: Option::from(to.to_string()),
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        }]
    };
    let events = vec![Event::new("Swap").add_attribute("AmountOut", amount_out.to_string())];
    Ok(ConvertStepResponse {
        amount: amount_out,
        massages: Some(messages),
        events: Some(events),
    })
}

fn swap_to_astro(
    deps: DepsMut,
    cfg: Config,
    token: AssetInfo,
    amount_in: Uint128,
) -> StdResult<ConvertStepResponse> {
    swap(
        deps,
        cfg.clone(),
        token,
        AssetInfo::Token {
            contract_addr: cfg.astro_token_contract,
        },
        amount_in,
        cfg.staking_contract,
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetFactory {} => to_binary(&query_get_factory(deps)?),
    }
}

fn query_get_factory(deps: Deps) -> StdResult<QueryAddressResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(QueryAddressResponse {
        address: config.factory_contract,
    })
}
