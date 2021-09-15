use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo,
    Reply, ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryConfigResponse, QueryMsg};
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
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Convert { asset_infos } => convert(deps, env, asset_infos),
        ExecuteMsg::ConvertMultiple { asset_infos } => {
            convert_multiple(deps, env, asset_infos, true)
        }
    }
}

pub fn convert_multiple(
    deps: DepsMut,
    env: Env,
    mut asset_infos: Vec<[AssetInfo; 2]>,
    assert_empty_state: bool,
) -> Result<Response, ContractError> {
    if assert_empty_state {
        let exists = CONVERT_MULTIPLE.load(deps.storage).unwrap_or_default();
        if exists.is_some() {
            return Err(ContractError::RepetitiveReply {});
        }
    }

    // Pop first item from asset_infos to convert
    let asset_to_convert = asset_infos.swap_remove(0);

    if asset_infos.len() > 0 {
        let asset_to_store = Some(ExecuteOnReply {
            asset_infos: asset_infos.clone(),
        });

        CONVERT_MULTIPLE.save(deps.storage, &asset_to_store)?;
    } else {
        CONVERT_MULTIPLE.remove(deps.storage);
    };

    let mut resp = convert(deps, env, asset_to_convert)?;
    if asset_infos.len() > 0 && resp.messages.len() > 0 {
        resp.messages.last_mut().unwrap().reply_on = ReplyOn::Success;
    }

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    let asset_infos = CONVERT_MULTIPLE.load(deps.storage)?;
    if asset_infos.is_none() {
        return Ok(Response::new());
    }

    convert_multiple(deps, env, asset_infos.unwrap().asset_infos, false)
}

fn convert(
    deps: DepsMut,
    env: Env,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let mut response = Response::default();

    // get pair lp token
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract.to_string(),
        &asset_infos,
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
            funds: vec![],
        }),
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    // swap tokens to astro
    let messages = convert_tokens(deps, cfg, assets.clone()).unwrap();
    for msg in messages {
        response.messages.push(SubMsg {
            msg: CosmosMsg::Wasm(msg),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }

    response.events.push(
        Event::new("LogConvert").add_attribute("assets", format!("{}, {}", assets[0], assets[1])),
    );

    Ok(response)
}

fn convert_tokens(mut deps: DepsMut, cfg: Config, assets: [Asset; 2]) -> StdResult<Vec<WasmMsg>> {
    let astro = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    // Merge assets, in case both are same
    let assets_reduced = if assets[0].info.equal(&assets[1].info) {
        vec![Asset {
            info: assets[0].info.clone(),
            amount: assets[0].amount.checked_add(assets[1].amount)?,
        }]
    } else {
        assets.to_vec()
    };

    let mut messages = vec![];

    for a in assets_reduced {
        let msgs = if a.info.equal(&astro) {
            // Transfer astro directly
            transfer_astro(&cfg, a.amount)
        } else {
            // Swap to astro and transfer to staking
            swap(
                deps.branch(),
                cfg.clone(),
                a.info,
                astro.clone(),
                a.amount,
                cfg.staking_contract.clone(),
            )
        };

        messages.extend(msgs.unwrap());
    }

    Ok(messages)
}

fn transfer_astro(cfg: &Config, amount: Uint128) -> StdResult<Vec<WasmMsg>> {
    let messages = vec![WasmMsg::Execute {
        contract_addr: cfg.astro_token_contract.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: cfg.staking_contract.to_string(),
            amount,
        })?,
        funds: vec![],
    }];

    Ok(messages)
}

fn swap(
    deps: DepsMut,
    cfg: Config,
    from_token: AssetInfo,
    to_token: AssetInfo,
    amount_in: Uint128,
    to: Addr,
) -> StdResult<Vec<WasmMsg>> {
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

    Ok(messages)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_get_config(deps)?),
    }
}

fn query_get_config(deps: Deps) -> StdResult<QueryConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(QueryConfigResponse {
        owner: config.owner,
        factory_contract: config.factory_contract,
        staking_contract: config.staking_contract,
        astro_token_contract: config.astro_token_contract,
    })
}
