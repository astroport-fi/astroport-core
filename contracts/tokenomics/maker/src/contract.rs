use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryConfigResponse, QueryMsg};
use crate::state::{Config, CONFIG};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairsResponse;
use astroport::pair::{Cw20HookMsg, SimulationResponse};
use astroport::querier::{query_pair_info, query_pairs_info};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, Deps, DepsMut, Env, Event, MessageInfo, Response,
    StdResult, SubMsg, Uint128, Uint64, WasmMsg,
};
use std::collections::HashMap;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.governance_percent > Uint64::new(100) {
        return Err(ContractError::IncorrectGovernancePercent {});
    };

    let cfg = Config {
        owner: info.sender,
        factory_contract: deps.api.addr_validate(&msg.factory_contract)?,
        staking_contract: deps.api.addr_validate(&msg.staking_contract)?,
        governance_contract: deps.api.addr_validate(&msg.governance_contract)?,
        governance_percent: msg.governance_percent,
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
        ExecuteMsg::SetConfig { governance_percent } => set_config(deps, env, governance_percent),
    }
}

fn collect(
    deps: DepsMut,
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
        if !balance.is_zero() {
            if a.equal(&astro) {
                // Transfer astro directly
                response
                    .messages
                    .append(&mut transfer_astro(deps.as_ref(), &cfg, balance)?);
            } else {
                // Swap to astro and transfer to staking and governance
                response.messages.append(&mut swap_to_and_transfer_astro(
                    deps.as_ref(),
                    &cfg,
                    a,
                    balance,
                )?);
            };
        }
    }

    Ok(response)
}

fn transfer_astro(deps: Deps, cfg: &Config, amount: Uint128) -> Result<Vec<SubMsg>, ContractError> {
    let mut result = vec![];

    let info = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let governance_amount =
        amount.multiply_ratio(Uint128::from(cfg.governance_percent), Uint128::new(100));
    let staking_amount = amount - governance_amount;

    let to_staking_asset = Asset {
        info: info.clone(),
        amount: staking_amount,
    };
    result.push(SubMsg::new(
        to_staking_asset.into_msg(&deps.querier, cfg.staking_contract.clone())?,
    ));

    let to_governance_asset = Asset {
        info,
        amount: governance_amount,
    };
    result.push(SubMsg::new(
        to_governance_asset.into_msg(&deps.querier, cfg.governance_contract.clone())?,
    ));

    Ok(result)
}

fn swap_to_and_transfer_astro(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<Vec<SubMsg>, ContractError> {
    let mut result = vec![];

    let to_token = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), to_token.clone()],
    )
    .map_err(|_| ContractError::PairNotFound(from_token.clone(), to_token.clone()))?;

    let msg = astroport::pair::QueryMsg::Simulation {
        offer_asset: Asset {
            info: from_token.clone(),
            amount: amount_in,
        },
    };
    let res: SimulationResponse = deps.querier.query_wasm_smart(&pair.contract_addr, &msg)?;
    let amount_out = res.return_amount;

    result.push(if from_token.is_native_token() {
        SubMsg::new(WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset: Asset {
                    info: from_token.clone(),
                    amount: amount_in,
                },
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
            funds: vec![Coin {
                denom: from_token.to_string(),
                amount: amount_in,
            }],
        })
    } else {
        SubMsg::new(WasmMsg::Execute {
            contract_addr: from_token.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pair.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    });

    result.append(&mut transfer_astro(deps, cfg, amount_out)?);

    Ok(result)
}

fn set_config(
    deps: DepsMut,
    _env: Env,
    governance_percent: Uint64,
) -> Result<Response, ContractError> {
    if governance_percent > Uint64::new(100) {
        return Err(ContractError::IncorrectGovernancePercent {});
    };

    CONFIG.update::<_, ContractError>(deps.storage, |mut v| {
        v.governance_percent = governance_percent;
        Ok(v)
    })?;

    Ok(Response::new().add_event(Event::new("Set config")))
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
        governance_contract: config.governance_contract,
        governance_percent: config.governance_percent,
        astro_token_contract: config.astro_token_contract,
    })
}
