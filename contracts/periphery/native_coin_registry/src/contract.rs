#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Bound;
use std::collections::HashSet;

use crate::error::ContractError;
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::native_coin_registry::{
    CoinResponse, Config, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, COINS_INFO,
};

/// version info for migration info
const CONTRACT_NAME: &str = "astroport-native-coin-registry";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Settings for pagination.
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(msg.owner.as_str())?,
        },
    )?;

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
        ExecuteMsg::Add { native_coins } => update(deps, info, native_coins),
        ExecuteMsg::Remove { native_coins } => remove(deps, info, native_coins),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
    }
}

/// Adds or updates a native asset in the registry.
///
/// * **native_coins** is a vector with the assets we are adding to the registry.
///
/// ## Executor
/// Only the owner can execute this.
pub fn update(
    deps: DepsMut,
    info: MessageInfo,
    native_coins: Vec<(String, u8)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Check for duplicate native coins
    let mut uniq = HashSet::new();
    if !native_coins.iter().all(|a| uniq.insert(&a.0)) {
        return Err(ContractError::DuplicateCoins {});
    }

    for (coin, decimals) in native_coins {
        if decimals == 0 {
            return Err(ContractError::CoinWithZeroPrecision(coin));
        }

        COINS_INFO.save(deps.storage, coin, &decimals)?;
    }

    Ok(Response::new().add_attributes(vec![attr("action", "add")]))
}

/// Removes an existing native asset from the registry.
///
/// * **native_coins** is a vector with the assets we are removing from the contract.
///
/// ## Executor
/// Only the owner can execute this.
pub fn remove(
    deps: DepsMut,
    info: MessageInfo,
    native_coins: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Check for duplicate native coins
    let mut uniq = HashSet::new();
    if !native_coins.iter().all(|a| uniq.insert(a)) {
        return Err(ContractError::DuplicateCoins {});
    }

    for coin in native_coins {
        if COINS_INFO.has(deps.storage, coin.clone()) {
            COINS_INFO.remove(deps.storage, coin);
        } else {
            return Err(ContractError::CoinDoesNotExist(coin));
        }
    }

    Ok(Response::new().add_attributes(vec![attr("action", "remove")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&CONFIG.load(deps.storage)?)?),
        QueryMsg::NativeToken { denom } => Ok(to_binary(&COINS_INFO.load(deps.storage, denom)?)?),
        QueryMsg::NativeTokens { start_after, limit } => {
            to_binary(&query_native_tokens(deps, start_after, limit)?)
        }
    }
}

/// Returns a vector with native assets by specified parameters.
pub fn query_native_tokens(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<CoinResponse>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    COINS_INFO
        .range(deps.storage, start, None, Order::Ascending)
        .map(|pair| {
            let (denom, decimals) = pair?;
            Ok(CoinResponse { denom, decimals })
        })
        .take(limit)
        .collect::<StdResult<Vec<CoinResponse>>>()
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-native-coin-registry" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
