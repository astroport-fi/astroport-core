use cosmwasm_std::{
    log, to_binary, Api, Binary, CanonicalAddr, CosmosMsg, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, InitResponse, MigrateResponse, MigrateResult, Querier, StdError,
    StdResult, Storage, WasmMsg,
};

use crate::querier::query_liquidity_token;
use crate::state::{
    read_config, read_pair, read_pairs, remove_pair, store_config, store_pair, Config,
};

use terraswap::asset::{AssetInfo, WeightedAssetInfo};
use terraswap::factory::{
    ConfigResponse, FactoryPairInfo, FactoryPairInfoRaw, HandleMsg, InitMsg, MigrateMsg,
    PairsResponse, QueryMsg,
};
use terraswap::hook::InitHook;
use terraswap::pair::InitMsg as PairInitMsg;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let config = Config {
        owner: deps.api.canonical_address(&env.message.sender)?,
        token_code_id: msg.token_code_id,
        pair_code_id: msg.pair_code_id,
    };

    store_config(&mut deps.storage, &config)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Some(hook) = msg.init_hook {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hook.contract_addr,
            msg: hook.msg,
            send: vec![],
        }));
    }

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> HandleResult {
    match msg {
        HandleMsg::UpdateConfig {
            owner,
            token_code_id,
            pair_code_id,
        } => try_update_config(deps, env, owner, token_code_id, pair_code_id),
        HandleMsg::CreatePair {
            asset_infos,
            init_hook,
            start_time,
            end_time,
            description,
        } => try_create_pair(
            deps,
            env,
            asset_infos,
            start_time,
            end_time,
            description,
            init_hook,
        ),
        HandleMsg::Register { asset_infos } => try_register(deps, env, asset_infos),
        HandleMsg::Unregister { asset_infos } => try_unregister(deps, env, asset_infos),
    }
}

// Only owner can execute it
pub fn try_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    token_code_id: Option<u64>,
    pair_code_id: Option<u64>,
) -> HandleResult {
    let mut config: Config = read_config(&deps.storage)?;

    // permission check
    if deps.api.canonical_address(&env.message.sender)? != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(token_code_id) = token_code_id {
        config.token_code_id = token_code_id;
    }

    if let Some(pair_code_id) = pair_code_id {
        config.pair_code_id = pair_code_id;
    }

    store_config(&mut deps.storage, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}
#[allow(clippy::too_many_arguments)]
// Anyone can execute it to create swap pair
pub fn try_create_pair<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_infos: [WeightedAssetInfo; 2],
    start_time: u64,
    end_time: u64,
    description: Option<String>,
    init_hook: Option<InitHook>,
) -> HandleResult {
    let config: Config = read_config(&deps.storage)?;

    let raw_infos = [
        asset_infos[0].info.to_raw(&deps)?,
        asset_infos[1].info.to_raw(&deps)?,
    ];
    if read_pair(&deps.storage, &raw_infos).is_ok() {
        return Err(StdError::generic_err("Pair already exists"));
    }

    let raw_asset_infos = [asset_infos[0].to_raw(&deps)?, asset_infos[1].to_raw(&deps)?];

    store_pair(
        &mut deps.storage,
        &FactoryPairInfoRaw {
            owner: deps.api.canonical_address(&env.message.sender)?,
            liquidity_token: CanonicalAddr::default(),
            contract_addr: CanonicalAddr::default(),
            asset_infos: raw_asset_infos,
            start_time,
            end_time,
        },
    )?;

    let mut messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
        code_id: config.pair_code_id,
        send: vec![],
        label: None,
        msg: to_binary(&PairInitMsg {
            asset_infos: asset_infos.clone(),
            token_code_id: config.token_code_id,
            init_hook: Some(InitHook {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::Register {
                    asset_infos: asset_infos.clone(),
                })?,
            }),
            start_time,
            end_time,
            description,
        })?,
    })];

    if let Some(hook) = init_hook {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hook.contract_addr,
            msg: hook.msg,
            send: vec![],
        }));
    }

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "create_pair"),
            log("pair", format!("{}-{}", asset_infos[0], asset_infos[1])),
        ],
        data: None,
    })
}
/// create pair execute this message
pub fn try_register<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_infos: [WeightedAssetInfo; 2],
) -> HandleResult {
    let raw_infos = [
        asset_infos[0].info.to_raw(&deps)?,
        asset_infos[1].info.to_raw(&deps)?,
    ];
    let pair_info: FactoryPairInfoRaw = read_pair(&deps.storage, &raw_infos)?;
    if pair_info.contract_addr != CanonicalAddr::default() {
        return Err(StdError::generic_err("Pair was already registered"));
    }

    let pair_contract = env.message.sender;
    let liquidity_token = query_liquidity_token(&deps, &pair_contract)?;
    store_pair(
        &mut deps.storage,
        &FactoryPairInfoRaw {
            contract_addr: deps.api.canonical_address(&pair_contract)?,
            liquidity_token: deps.api.canonical_address(&liquidity_token)?,
            ..pair_info
        },
    )?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "register"),
            log("pair_contract_addr", pair_contract),
        ],
        data: None,
    })
}
/// remove from list of pairs
pub fn try_unregister<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_infos: [AssetInfo; 2],
) -> HandleResult {
    let raw_infos = [asset_infos[0].to_raw(&deps)?, asset_infos[1].to_raw(&deps)?];

    let pair_info: FactoryPairInfoRaw = read_pair(&deps.storage, &raw_infos)?;

    // Permission check
    if pair_info.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    remove_pair(&mut deps.storage, &pair_info);

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "unregister"),
            log("pair", format!("{}-{}", asset_infos[0], asset_infos[1])),
        ],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Pair { asset_infos } => to_binary(&query_pair(deps, asset_infos)?),
        QueryMsg::Pairs { start_after, limit } => {
            to_binary(&query_pairs(deps, start_after, limit)?)
        }
    }
}

pub fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let state: Config = read_config(&deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.human_address(&state.owner)?,
        token_code_id: state.token_code_id,
        pair_code_id: state.pair_code_id,
    };

    Ok(resp)
}

pub fn query_pair<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    asset_infos: [AssetInfo; 2],
) -> StdResult<FactoryPairInfo> {
    let raw_infos = [asset_infos[0].to_raw(&deps)?, asset_infos[1].to_raw(&deps)?];
    let pair_info: FactoryPairInfoRaw = read_pair(&deps.storage, &raw_infos)?;
    pair_info.to_normal(&deps)
}

pub fn query_pairs<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    let start_after = if let Some(start_after) = start_after {
        Some([start_after[0].to_raw(&deps)?, start_after[1].to_raw(&deps)?])
    } else {
        None
    };

    let pairs: Vec<FactoryPairInfo> = read_pairs(&deps, start_after, limit)?;
    let resp = PairsResponse { pairs };

    Ok(resp)
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
