use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Reply,
    ReplyOn, Response, StdError, StdResult, SubMsg, WasmMsg,
};

use crate::error::ContractError;
use crate::migration;
use crate::querier::query_pair_info;

use crate::state::{
    pair_key, read_pairs, Config, TmpPairInfo, CONFIG, OWNERSHIP_PROPOSAL, PAIRS, PAIR_CONFIGS,
    TMP_PAIR_INFO,
};

use crate::response::MsgInstantiateContractResponse;

use astroport::asset::{addr_validate_to_lower, AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, MigrateMsg, PairConfig, PairType,
    PairsResponse, QueryMsg,
};

use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::pair::InstantiateMsg as PairInstantiateMsg;
use cw2::{get_contract_version, set_contract_version};
use protobuf::Message;
use std::collections::HashSet;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-factory";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID of sub-message.
const INSTANTIATE_PAIR_REPLY_ID: u64 = 1;

/// ## Description
/// Creates a new contract with the specified parameters in the `msg` variable.
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`]
///
/// * **_info** is the object of type [`MessageInfo`]
///
/// * **msg**  is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        token_code_id: msg.token_code_id,
        fee_address: None,
        generator_address: None,
        whitelist_code_id: msg.whitelist_code_id,
    };

    if let Some(generator_address) = msg.generator_address {
        config.generator_address = Some(addr_validate_to_lower(
            deps.api,
            generator_address.as_str(),
        )?);
    }

    if let Some(fee_address) = msg.fee_address {
        config.fee_address = Some(addr_validate_to_lower(deps.api, fee_address.as_str())?);
    }

    let config_set: HashSet<String> = msg
        .pair_configs
        .iter()
        .map(|pc| pc.pair_type.to_string())
        .collect();

    if config_set.len() != msg.pair_configs.len() {
        return Err(ContractError::PairConfigDuplicate {});
    }

    for pc in msg.pair_configs.iter() {
        // validate total and maker fee bps
        if !pc.valid_fee_bps() {
            return Err(ContractError::PairConfigInvalidFeeBps {});
        }
        PAIR_CONFIGS.save(deps.storage, pc.clone().pair_type.to_string(), pc)?;
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// ## Description
/// Data structure for update the settings of the factory contract.
pub struct UpdateConfig {
    /// Sets CW20 token contract code identifier
    token_code_id: Option<u64>,
    /// Sets contract address to send fees to
    fee_address: Option<String>,
    /// Sets contract address that used for auto_stake from pools
    generator_address: Option<String>,
    /// cw1 whitelist contract code id used to store 3rd party rewards in pools
    whitelist_code_id: Option<u64>,
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig {
///             token_code_id,
///             fee_address,
///             generator_address,
///         }** Updates general settings.
///
/// * **ExecuteMsg::UpdatePairConfig { config }** Updates pair configuration.
///
/// * **ExecuteMsg::CreatePair {
///             pair_type,
///             asset_infos,
///             init_params,
///         }** Creates a new pair with the specified input parameters
///
/// * **ExecuteMsg::Deregister { asset_infos }** Removes a exists pair with the specified input parameters.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Approves ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            token_code_id,
            fee_address,
            generator_address,
            whitelist_code_id,
        } => execute_update_config(
            deps,
            env,
            info,
            UpdateConfig {
                token_code_id,
                fee_address,
                generator_address,
                whitelist_code_id,
            },
        ),
        ExecuteMsg::UpdatePairConfig { config } => execute_update_pair_config(deps, info, config),
        ExecuteMsg::CreatePair {
            pair_type,
            asset_infos,
            init_params,
        } => execute_create_pair(deps, env, pair_type, asset_infos, init_params),
        ExecuteMsg::Deregister { asset_infos } => deregister(deps, info, asset_infos),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
    }
}

/// ## Description
/// Updates general settings. Returns an [`ContractError`] on failure or the following [`Config`]
/// data will be updated if successful.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **param** is the object of type [`UpdateConfig`] that contains information to update.
///
/// ##Executor
/// Only owner can execute it
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    param: UpdateConfig,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(fee_address) = param.fee_address {
        // validate address format
        config.fee_address = Some(addr_validate_to_lower(deps.api, fee_address.as_str())?);
    }

    if let Some(generator_address) = param.generator_address {
        // validate address format
        config.generator_address = Some(addr_validate_to_lower(
            deps.api,
            generator_address.as_str(),
        )?);
    }

    if let Some(token_code_id) = param.token_code_id {
        config.token_code_id = token_code_id;
    }

    if let Some(code_id) = param.whitelist_code_id {
        config.whitelist_code_id = code_id;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// ## Description
/// Updates pair configuration. Returns an [`ContractError`] on failure or
/// the following [`PairConfig`] data will be updated if successful.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **info** is the object of type [`MessageInfo`]
///
/// * **pair_config** is the object of type [`PairConfig`] that contains information to update.
///
/// ## Executor
/// Only owner can execute it
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

    // validate total and maker fee bps
    if !pair_config.valid_fee_bps() {
        return Err(ContractError::PairConfigInvalidFeeBps {});
    }

    PAIR_CONFIGS.save(
        deps.storage,
        pair_config.pair_type.to_string(),
        &pair_config,
    )?;

    Ok(Response::new().add_attribute("action", "update_pair_config"))
}

/// ## Description
/// Creates a new pair with the specified parameters in the `asset_infos` variable. Returns an [`ContractError`] on failure or
/// returns the address of the contract if the creation was successful.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **pair_type** is the object of type [`PairType`].
///
/// * **asset_infos** is an array with two items the type of [`AssetInfo`].
///
/// * **init_params** is an [`Option`] type. Receive a binary data.
pub fn execute_create_pair(
    deps: DepsMut,
    env: Env,
    pair_type: PairType,
    asset_infos: [AssetInfo; 2],
    init_params: Option<Binary>,
) -> Result<Response, ContractError> {
    asset_infos[0].check(deps.api)?;
    asset_infos[1].check(deps.api)?;

    if asset_infos[0] == asset_infos[1] {
        return Err(ContractError::DoublingAssets {});
    }

    let config = CONFIG.load(deps.storage)?;

    if PAIRS
        .may_load(deps.storage, &pair_key(&asset_infos))?
        .is_some()
    {
        return Err(ContractError::PairWasCreated {});
    }

    // Get pair type from config
    let pair_config = PAIR_CONFIGS
        .load(deps.storage, pair_type.to_string())
        .map_err(|_| ContractError::PairConfigNotFound {})?;

    // Check if pair config is disabled
    if pair_config.is_disabled.is_some() && pair_config.is_disabled.unwrap() {
        return Err(ContractError::PairConfigDisabled {});
    }

    let pair_key = pair_key(&asset_infos);
    TMP_PAIR_INFO.save(deps.storage, &TmpPairInfo { pair_key })?;

    let sub_msg: Vec<SubMsg> = vec![SubMsg {
        id: INSTANTIATE_PAIR_REPLY_ID,
        msg: WasmMsg::Instantiate {
            admin: Some(config.owner.to_string()),
            code_id: pair_config.code_id,
            msg: to_binary(&PairInstantiateMsg {
                asset_infos: asset_infos.clone(),
                token_code_id: config.token_code_id,
                factory_addr: env.contract.address.to_string(),
                init_params,
            })?,
            funds: vec![],
            label: "Astroport pair".to_string(),
        }
        .into(),
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new()
        .add_submessages(sub_msg)
        .add_attributes(vec![
            attr("action", "create_pair"),
            attr("pair", format!("{}-{}", asset_infos[0], asset_infos[1])),
        ]))
}

/// # Description
/// The entry point to the contract for processing the reply from the submessage
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`Reply`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let tmp = TMP_PAIR_INFO.load(deps.storage)?;
    if PAIRS.may_load(deps.storage, &tmp.pair_key)?.is_some() {
        return Err(ContractError::PairWasRegistered {});
    }

    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    let pair_contract = addr_validate_to_lower(deps.api, res.get_contract_address())?;

    PAIRS.save(deps.storage, &tmp.pair_key, &pair_contract)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "register"),
        attr("pair_contract_addr", pair_contract),
    ]))
}

/// ## Description
/// Removes a exists pair with the specified parameters in the `asset_infos` variable.
/// Returns an [`ContractError`] on failure or returns the [`Response`] with the specified attributes
/// if the operation was successful.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **asset_infos** are an array with two items the type of [`AssetInfo`].
///
/// ## Executor
/// Only owner can execute it
pub fn deregister(
    deps: DepsMut,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
) -> Result<Response, ContractError> {
    asset_infos[0].check(deps.api)?;
    asset_infos[1].check(deps.api)?;

    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let pair_addr: Addr = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    PAIRS.remove(deps.storage, &pair_key(&asset_infos));

    Ok(Response::new().add_attributes(vec![
        attr("action", "deregister"),
        attr("pair_contract_addr", pair_addr),
    ]))
}

/// ## Description
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns controls settings that specified in custom [`ConfigResponse`] structure.
///
/// * **QueryMsg::Pair { asset_infos }** Returns the [`PairInfo`] object with the specified input parameters
///
/// * **QueryMsg::Pairs { start_after, limit }** Returns an array that contains items of [`PairInfo`]
/// according to the specified input parameters.
///
/// * **QueryMsg::FeeInfo { pair_type }** Returns the settings specified in the custom
/// structure [`FeeInfoResponse`].
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

/// ## Description
/// Returns controls settings that specified in custom [`ConfigResponse`] structure
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
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
        fee_address: config.fee_address,
        generator_address: config.generator_address,
        whitelist_code_id: config.whitelist_code_id,
    };

    Ok(resp)
}

/// ## Description
/// Returns a pair with the specified parameters in the `asset_infos` variable.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **asset_infos** it is array with two items the type of [`AssetInfo`].
pub fn query_pair(deps: Deps, asset_infos: [AssetInfo; 2]) -> StdResult<PairInfo> {
    let pair_addr = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    query_pair_info(deps, &pair_addr)
}

/// ## Description
/// Returns an array that contains items of [`PairInfo`] according to the specified parameters in `start_after` and `limit` variables.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field that accepts an array with two items the type of [`AssetInfo`].
///
/// * **limit** is a [`Option`] type. Sets the number of items to be read.
pub fn query_pairs(
    deps: Deps,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    let pairs: Vec<PairInfo> = read_pairs(deps, start_after, limit)
        .iter()
        .map(|pair_addr| query_pair_info(deps, pair_addr).unwrap())
        .collect();

    Ok(PairsResponse { pairs })
}

/// ## Description
/// Returns the settings specified in the custom structure [`FeeInfoResponse`] for the specified parameters in the `pair_type` variable.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **pair_type** is the type of pair available in [`PairType`]
pub fn query_fee_info(deps: Deps, pair_type: PairType) -> StdResult<FeeInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let pair_config = PAIR_CONFIGS.load(deps.storage, pair_type.to_string())?;

    Ok(FeeInfoResponse {
        fee_address: config.fee_address,
        total_fee_bps: pair_config.total_fee_bps,
        maker_fee_bps: pair_config.maker_fee_bps,
    })
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-factory" => match contract_version.version.as_ref() {
            "1.0.0" => {
                let config_v100 = migration::CONFIGV100.load(deps.storage)?;

                let new_config = Config {
                    whitelist_code_id: msg.whitelist_code_id,
                    fee_address: config_v100.fee_address,
                    generator_address: config_v100.generator_address,
                    owner: config_v100.owner,
                    token_code_id: config_v100.token_code_id,
                };

                CONFIG.save(deps.storage, &new_config)?;
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
