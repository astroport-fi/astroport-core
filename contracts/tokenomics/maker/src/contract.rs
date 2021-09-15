use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryConfigResponse, QueryMsg};
use crate::state::{Config, CONFIG};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairsResponse;
use astroport::pair::Cw20HookMsg;
use astroport::querier::{query_pair_info, query_pairs_info};
use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use std::collections::HashMap;

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
        ExecuteMsg::Collect { start_after, limit } => collect(deps, env, start_after, limit),
    }
}

fn collect(
    mut deps: DepsMut,
    env: Env,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let pairs_info: PairsResponse = query_pairs_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        start_after,
        limit,
    )?;

    let pairs_info: Vec<AssetInfo> = pairs_info
        .pairs
        .into_iter()
        .map(|p| p.asset_infos)
        .into_iter()
        .flatten()
        .collect();

    let mut assets_map: HashMap<String, AssetInfo> = HashMap::new();
    for p in pairs_info {
        assets_map.insert(p.to_string(), p);
    }

    let astro = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let mut response = Response::default();

    for a in assets_map.into_values() {
        // Get Balance
        let balance = a.query_pool(&deps.querier, env.contract.address.clone())?;

        let msg = if a.equal(&astro) {
            // Transfer astro directly
            let asset = Asset {
                info: a,
                amount: balance,
            };

            asset.into_msg(&deps.querier, cfg.staking_contract.clone())?
        } else {
            // Swap to astro and transfer to staking
            swap(
                deps.branch(),
                cfg.clone(),
                a,
                astro.clone(),
                balance,
                cfg.staking_contract.clone(),
            )?
        };

        response.messages.push(SubMsg {
            msg,
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }

    Ok(response)
}

fn swap(
    deps: DepsMut,
    cfg: Config,
    from_token: AssetInfo,
    to_token: AssetInfo,
    amount_in: Uint128,
    to: Addr,
) -> StdResult<CosmosMsg> {
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract,
        &[from_token.clone(), to_token],
    )?;

    let msg = if from_token.is_native_token() {
        WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset: Asset {
                    info: from_token.clone(),
                    amount: amount_in,
                },
                belief_price: None,
                max_spread: None,
                to: Option::from(to.to_string()),
            })?,
            funds: vec![Coin {
                denom: from_token.to_string(),
                amount: amount_in,
            }],
        }
    } else {
        WasmMsg::Execute {
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
        }
    };

    Ok(CosmosMsg::Wasm(msg))
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
