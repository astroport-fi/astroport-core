use std::collections::{BTreeSet, HashSet};
use std::ops::RangeInclusive;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, ensure, to_json_binary, BankMsg, Binary, Deps, DepsMut, Empty, Env, Event, MessageInfo,
    Order, Response, StdError, StdResult, Storage,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Bound;
use itertools::Itertools;

use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::native_coin_registry::{
    CoinResponse, Config, ExecuteMsg, InstantiateMsg, QueryMsg, COINS_INFO,
};

use crate::error::ContractError;
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL};

/// version info for migration
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Settings for pagination.
pub const DEFAULT_LIMIT: u32 = 50;
/// Allowed decimals
pub const ALLOWED_DECIMALS: RangeInclusive<u8> = 0..=18u8;

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
            owner: deps.api.addr_validate(&msg.owner)?,
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
        ExecuteMsg::Add { native_coins } => update_decimals(deps, info, native_coins),
        ExecuteMsg::Register { native_coins } => register_decimals(deps, info, native_coins),
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

/// Register a native asset in the registry.
/// Sender must send any number of coins per each asset added.
/// All funds will be returned to the sender.
/// Permissionless.
///
/// * **native_coins** is a vector with the assets we are adding to the registry.
pub fn register_decimals(
    deps: DepsMut,
    info: MessageInfo,
    native_coins: Vec<(String, u8)>,
) -> Result<Response, ContractError> {
    let coins_map = info
        .funds
        .iter()
        .map(|coin| &coin.denom)
        .collect::<BTreeSet<_>>();

    for (denom, _) in &native_coins {
        coins_map
            .get(denom)
            .ok_or(ContractError::MustSendCoin(denom.clone()))?;
    }

    // Return the funds back to the sender
    let send_msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: info.funds,
    };

    inner_add(deps.storage, native_coins, Some(send_msg))
}

/// Adds or updates a native asset in the registry.
///
/// * **native_coins** is a vector with the assets we are adding to the registry.
///
/// ## Executor
/// Only the owner can execute this.
pub fn update_decimals(
    deps: DepsMut,
    info: MessageInfo,
    native_coins: Vec<(String, u8)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    inner_add(deps.storage, native_coins, None)
}

/// Function with shared logic for both permissioned and permissionless endpoints.
///
/// * **native_coins** is a vector with the assets with respective decimals we are adding to the registry.
/// * **maybe_send_msg** is an optional BankMsg to send funds back to the sender.
/// It also serves as a flag to differentiate between permissioned and permissionless endpoints.
pub fn inner_add(
    storage: &mut dyn Storage,
    native_coins: Vec<(String, u8)>,
    maybe_send_msg: Option<BankMsg>,
) -> Result<Response, ContractError> {
    // Check for duplicate native coins
    let mut uniq = HashSet::new();
    if !native_coins.iter().all(|a| uniq.insert(&a.0)) {
        return Err(ContractError::DuplicateCoins {});
    }

    native_coins.iter().try_for_each(|(denom, decimals)| {
        ensure!(
            ALLOWED_DECIMALS.contains(decimals),
            ContractError::InvalidDecimals {
                denom: denom.clone(),
                decimals: *decimals,
            }
        );

        COINS_INFO
            .update(storage, denom.clone(), |v| match v {
                Some(_) if maybe_send_msg.is_some() => {
                    Err(ContractError::CoinAlreadyExists(denom.clone()))
                }
                _ => Ok(*decimals),
            })
            .map(|_| ())
    })?;

    let coin_attrs = native_coins
        .iter()
        .map(|(coin, decimals)| attr(coin, decimals.to_string()));
    let event = Event::new("added_coins").add_attributes(coin_attrs);

    let response = Response::new()
        .add_attributes([("action", "add")])
        .add_event(event);

    if let Some(send_msg) = maybe_send_msg {
        Ok(response.add_message(send_msg))
    } else {
        Ok(response)
    }
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

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    // Check for duplicate native coins
    let mut uniq = HashSet::new();
    if !native_coins.iter().all(|a| uniq.insert(a)) {
        return Err(ContractError::DuplicateCoins {});
    }

    for denom in &native_coins {
        if COINS_INFO.has(deps.storage, denom.clone()) {
            COINS_INFO.remove(deps.storage, denom.clone());
        } else {
            return Err(ContractError::CoinDoesNotExist(denom.clone()));
        }
    }

    let removed_coins = native_coins.into_iter().join(", ");
    Ok(Response::new().add_attributes([("action", "remove"), ("coins", &removed_coins)]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::NativeToken { denom } => to_json_binary(&COINS_INFO.load(deps.storage, denom)?),
        QueryMsg::NativeTokens { start_after, limit } => {
            to_json_binary(&query_native_tokens(deps, start_after, limit)?)
        }
    }
}

/// Returns a vector with native assets by specified parameters.
pub fn query_native_tokens(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<CoinResponse>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    COINS_INFO
        .range(deps.storage, start, None, Order::Ascending)
        .map(|pair| pair.map(|(denom, decimals)| CoinResponse { denom, decimals }))
        .take(limit)
        .collect()
}

/// Manages contract migration.
#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-native-coin-registry" => match contract_version.version.as_ref() {
            "1.0.1" => {}
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
