use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::UpdateAddr;
use astroport::maker::{
    ExecuteMsg, InstantiateMsg, QueryBalancesResponse, QueryConfigResponse, QueryMsg,
};
use astroport::pair::{Cw20HookMsg, QueryMsg as PairQueryMsg};
use astroport::querier::query_pair_info;
use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, Coin, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Reply, ReplyOn, Response, StdResult, SubMsg, Uint128, Uint64, WasmMsg, WasmQuery,
};
use cw2::set_contract_version;
use std::collections::HashMap;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-maker";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let owner = deps.api.addr_validate(&msg.owner)?;

    let governance_contract = if let Some(governance_contract) = msg.governance_contract {
        Option::from(deps.api.addr_validate(&governance_contract)?)
    } else {
        None
    };

    let governance_percent = if let Some(governance_percent) = msg.governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };
        governance_percent
    } else {
        Uint64::zero()
    };

    let cfg = Config {
        owner,
        astro_token_contract: deps.api.addr_validate(&msg.astro_token_contract)?,
        factory_contract: deps.api.addr_validate(&msg.factory_contract)?,
        staking_contract: deps.api.addr_validate(&msg.staking_contract)?,
        governance_contract,
        governance_percent,
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
        ExecuteMsg::Collect { pair_addresses } => collect(deps, env, pair_addresses),
        ExecuteMsg::SetConfig {
            owner,
            staking_contract,
            governance_contract,
            governance_percent,
        } => set_config(
            deps,
            info,
            owner,
            staking_contract,
            governance_contract,
            governance_percent,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let astro = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let mut resp = Response::new();

    let balance = astro.query_pool(&deps.querier, env.contract.address)?;
    if !balance.is_zero() {
        resp.messages
            .append(&mut distribute_astro(deps.as_ref(), &cfg, balance)?);
    }

    Ok(resp)
}

fn collect(deps: DepsMut, env: Env, pair_addresses: Vec<Addr>) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let astro = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let mut response = Response::default();

    // Collect assets
    let mut assets_map: HashMap<String, AssetInfo> = HashMap::new();
    for pair in pair_addresses {
        let pair = query_pair(deps.as_ref(), pair)?;
        assets_map.insert(pair[0].to_string(), pair[0].clone());
        assets_map.insert(pair[1].to_string(), pair[1].clone());
    }

    // Swap all non-astro tokens
    for a in assets_map.values().cloned().filter(|a| a.ne(&astro)) {
        // Get Balance
        let balance = a.query_pool(&deps.querier, env.contract.address.clone())?;
        if !balance.is_zero() {
            // Swap to astro and transfer to staking and governance
            response
                .messages
                .push(swap_to_astro(deps.as_ref(), &cfg, a, balance)?);
        }
    }

    // Use ReplyOn to have a proper amount of astro
    if !response.messages.is_empty() {
        response.messages.last_mut().unwrap().reply_on = ReplyOn::Success;
    } else {
        let balance = astro.query_pool(&deps.querier, env.contract.address)?;
        if !balance.is_zero() {
            response
                .messages
                .append(&mut distribute_astro(deps.as_ref(), &cfg, balance)?);
        }
    }

    Ok(response)
}

fn distribute_astro(
    deps: Deps,
    cfg: &Config,
    amount: Uint128,
) -> Result<Vec<SubMsg>, ContractError> {
    let mut result = vec![];

    let info = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let governance_amount = if let Some(governance_contract) = cfg.governance_contract.clone() {
        let amount =
            amount.multiply_ratio(Uint128::from(cfg.governance_percent), Uint128::new(100));
        let to_governance_asset = Asset {
            info: info.clone(),
            amount,
        };
        result.push(SubMsg::new(
            to_governance_asset.into_msg(&deps.querier, governance_contract)?,
        ));
        amount
    } else {
        Uint128::zero()
    };
    let staking_amount = amount - governance_amount;
    let to_staking_asset = Asset {
        info,
        amount: staking_amount,
    };
    result.push(SubMsg::new(
        to_staking_asset.into_msg(&deps.querier, cfg.staking_contract.clone())?,
    ));
    Ok(result)
}

fn swap_to_astro(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    let to_token = AssetInfo::Token {
        contract_addr: cfg.astro_token_contract.clone(),
    };

    let pair: PairInfo = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), to_token.clone()],
    )
    .map_err(|_| ContractError::PairNotFound(from_token.clone(), to_token.clone()))?;

    if from_token.is_native_token() {
        let mut offer_asset = Asset {
            info: from_token.clone(),
            amount: amount_in,
        };

        // deduct tax first
        let amount_in = amount_in.checked_sub(offer_asset.compute_tax(&deps.querier)?)?;

        offer_asset.amount = amount_in;

        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: pair.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset,
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
            funds: vec![Coin {
                denom: from_token.to_string(),
                amount: amount_in,
            }],
        }))
    } else {
        Ok(SubMsg::new(WasmMsg::Execute {
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
        }))
    }
}

fn set_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    staking_contract: Option<String>,
    governance_contract: Option<UpdateAddr>,
    governance_percent: Option<Uint64>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "set_config")];

    let mut config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        // validate address format
        config.owner = deps.api.addr_validate(owner.as_str())?;
    }

    if let Some(staking_contract) = staking_contract {
        config.staking_contract = deps.api.addr_validate(&staking_contract)?;
        attributes.push(Attribute::new("staking_contract", &staking_contract));
    };

    if let Some(action) = governance_contract {
        match action {
            UpdateAddr::Set { address: gov } => {
                config.governance_contract = Option::from(deps.api.addr_validate(&gov)?);
                attributes.push(Attribute::new("governance_contract", &gov));
            }
            UpdateAddr::Remove {} => {
                config.governance_contract = None;
            }
        }
    }

    if let Some(governance_percent) = governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };

        config.governance_percent = governance_percent;
        attributes.push(Attribute::new("governance_percent", governance_percent));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_get_config(deps)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
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

fn query_get_balances(
    deps: Deps,
    env: Env,
    assets: Vec<AssetInfo>,
) -> StdResult<QueryBalancesResponse> {
    let mut resp = QueryBalancesResponse { balances: vec![] };

    for a in assets {
        // Get Balance
        let balance = a.query_pool(&deps.querier, env.contract.address.clone())?;
        if !balance.is_zero() {
            resp.balances.push(Asset {
                info: a,
                amount: balance,
            })
        }
    }

    Ok(resp)
}

pub fn query_pair(deps: Deps, contract_addr: Addr) -> StdResult<[AssetInfo; 2]> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&PairQueryMsg::Pair {})?,
    }))?;

    Ok(res.asset_infos)
}
