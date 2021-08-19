use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, ReplyOn,
    Response, StdError, StdResult, SubMsg, WasmMsg,
};

use crate::error::ContractError;
use crate::querier::query_liquidity_token;
use crate::state::{pair_key, read_pairs, Config, CONFIG, PAIRS, PAIR_CONFIGS};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, MigrateMsg, PairConfig, PairType,
    PairsResponse, QueryMsg,
};
use astroport::hook::InitHook;
use astroport::pair::InstantiateMsg as PairInstantiateMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        owner: info.sender,
        token_code_id: msg.token_code_id,
        fee_address: msg.fee_address.unwrap_or_else(|| Addr::unchecked("")),
    };

    for pc in msg.pair_configs.iter() {
        if PAIR_CONFIGS.has(deps.storage, pc.clone().pair_type.to_string()) {
            return Err(ContractError::PairConfigDuplicate {});
        }

        PAIR_CONFIGS.save(deps.storage, pc.clone().pair_type.to_string(), pc)?;
    }

    CONFIG.save(deps.storage, &config)?;

    let mut messages: Vec<SubMsg> = vec![];
    if let Some(hook) = msg.init_hook {
        messages.push(SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: hook.contract_addr,
                msg: hook.msg,
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }

    Ok(Response::new().add_submessages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            owner,
            token_code_id,
            fee_address,
        } => execute_update_config(deps, env, info, owner, token_code_id, fee_address),
        ExecuteMsg::UpdatePairConfig { config } => execute_update_pair_config(deps, info, config),
        ExecuteMsg::RemovePairConfig { pair_type } => {
            execute_remove_pair_config(deps, info, pair_type)
        }
        ExecuteMsg::CreatePair {
            pair_type,
            asset_infos,
            init_hook,
        } => execute_create_pair(deps, env, pair_type, asset_infos, init_hook),
        ExecuteMsg::Register { asset_infos } => register(deps, info, asset_infos),
        ExecuteMsg::Deregister { asset_infos } => deregister(deps, info, asset_infos),
    }
}

// Only owner can execute it
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<Addr>,
    token_code_id: Option<u64>,
    fee_address: Option<Addr>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        // validate address format
        config.owner = deps.api.addr_validate(owner.as_str())?;
    }

    if let Some(fee_address) = fee_address {
        // validate address format
        config.fee_address = deps.api.addr_validate(fee_address.as_str())?;
    }

    if let Some(token_code_id) = token_code_id {
        config.token_code_id = token_code_id;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

pub fn execute_update_pair_config(
    deps: DepsMut,
    info: MessageInfo,
    pair_config: PairConfig,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    PAIR_CONFIGS.save(
        deps.storage,
        pair_config.pair_type.to_string(),
        &pair_config,
    )?;

    Ok(Response::new().add_attribute("action", "update_pair_config"))
}

pub fn execute_remove_pair_config(
    deps: DepsMut,
    info: MessageInfo,
    pair_type: PairType,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if !PAIR_CONFIGS.has(deps.storage, pair_type.to_string()) {
        return Err(ContractError::PairConfigNotFound {});
    }

    PAIR_CONFIGS.remove(deps.storage, pair_type.to_string());

    Ok(Response::new().add_attribute("action", "remove_pair_config"))
}

// Anyone can execute it to create swap pair
pub fn execute_create_pair(
    deps: DepsMut,
    env: Env,
    pair_type: PairType,
    asset_infos: [AssetInfo; 2],
    init_hook: Option<InitHook>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if PAIRS
        .may_load(deps.storage, &pair_key(&asset_infos))
        .unwrap_or(None)
        .is_some()
    {
        return Err(StdError::generic_err("Pair already exists").into());
    }

    // Get pair type from config
    let pair_config = PAIR_CONFIGS
        .load(deps.storage, pair_type.to_string())
        .map_err(|_| ContractError::PairConfigNotFound {})?;

    PAIRS.save(
        deps.storage,
        &pair_key(&asset_infos),
        &PairInfo {
            liquidity_token: Addr::unchecked(""),
            contract_addr: Addr::unchecked(""),
            asset_infos: [asset_infos[0].clone(), asset_infos[1].clone()],
            pair_type: pair_type.clone(),
        },
    )?;

    let mut messages: Vec<SubMsg> = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            code_id: pair_config.code_id,
            funds: vec![],
            admin: None,
            label: String::from("Astroport pair"),
            msg: to_binary(&PairInstantiateMsg {
                asset_infos: asset_infos.clone(),
                token_code_id: config.token_code_id,
                init_hook: Some(InitHook {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecuteMsg::Register {
                        asset_infos: asset_infos.clone(),
                    })?,
                }),
                factory_addr: env.contract.address,
                pair_type,
            })?,
        }
        .into(),
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    }];

    if let Some(hook) = init_hook {
        messages.push(SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: hook.contract_addr,
                msg: hook.msg,
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![
            attr("action", "create_pair"),
            attr("pair", format!("{}-{}", asset_infos[0], asset_infos[1])),
        ]))
}

/// create pair execute this message
pub fn register(
    deps: DepsMut,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    let pair_info: PairInfo = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    if pair_info.contract_addr != Addr::unchecked("") {
        return Err(ContractError::PairWasRegistered {});
    }

    let pair_contract = info.sender;
    let liquidity_token = query_liquidity_token(deps.as_ref(), pair_contract.clone())?;
    PAIRS.save(
        deps.storage,
        &pair_key(&asset_infos),
        &PairInfo {
            contract_addr: pair_contract.clone(),
            liquidity_token,
            ..pair_info
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "register"),
        attr("pair_contract_addr", pair_contract),
    ]))
}

/// create pair execute this message
pub fn deregister(
    deps: DepsMut,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let pair_info: PairInfo = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    PAIRS.remove(deps.storage, &pair_key(&asset_infos));

    Ok(Response::new().add_attributes(vec![
        attr("action", "deregister"),
        attr("pair_contract_addr", pair_info.contract_addr),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Pair { asset_infos } => to_binary(&query_pair(deps, asset_infos)?),
        QueryMsg::Pairs { start_after, limit } => {
            to_binary(&query_pairs(deps, start_after, limit)?)
        }
        QueryMsg::FeeInfo { pair_type } => to_binary(&query_fee_info(deps, pair_type)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        owner: config.owner,
        token_code_id: config.token_code_id,
        pair_configs: PAIR_CONFIGS
            .range(deps.storage, None, None, Order::Ascending)
            .map(|item| {
                let (_, cfg) = item.unwrap();
                cfg
            })
            .collect(),
    };

    Ok(resp)
}

pub fn query_pair(deps: Deps, asset_infos: [AssetInfo; 2]) -> StdResult<PairInfo> {
    PAIRS.load(deps.storage, &pair_key(&asset_infos))
}

pub fn query_pairs(
    deps: Deps,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    let pairs: Vec<PairInfo> = read_pairs(deps, start_after, limit);
    let resp = PairsResponse { pairs };

    Ok(resp)
}

pub fn query_fee_info(deps: Deps, pair_type: PairType) -> StdResult<FeeInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let pair_config = PAIR_CONFIGS.load(deps.storage, pair_type.to_string())?;

    Ok(FeeInfoResponse {
        fee_address: Some(config.fee_address).filter(|a| a != &Addr::unchecked("")),
        total_fee_bps: pair_config.total_fee_bps,
        maker_fee_bps: pair_config.maker_fee_bps,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
