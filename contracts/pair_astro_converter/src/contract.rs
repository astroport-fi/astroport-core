use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, ensure, from_json, to_json_binary, wasm_execute, Addr, DepsMut, Empty, Env,
    MessageInfo, Response,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::Cw20ReceiveMsg;

use astroport::asset::{addr_opt_validate, Asset, AssetInfo, AssetInfoExt};
use astroport::astro_converter;
use astroport::pair::{Cw20HookMsg, ExecuteMsg};

use crate::error::ContractError;
use crate::migration::{migrate_config, sanity_checks, MigrateMsg};
use crate::state::CONFIG;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> Result<Response, ContractError> {
    unimplemented!("{CONTRACT_NAME} cannot be instantiated");
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs a swap operation with the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, info, msg),
        ExecuteMsg::Swap {
            offer_asset, to, ..
        } => {
            ensure!(
                offer_asset.is_native_token(),
                ContractError::Cw20DirectSwap {}
            );
            offer_asset.assert_sent_native_token_balance(&info)?;
            let sender = info.sender.clone();

            swap(deps, sender, offer_asset, to)
        }
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Swap { to, .. } => swap(
            deps,
            Addr::unchecked(cw20_msg.sender),
            AssetInfo::cw20_unchecked(info.sender).with_balance(cw20_msg.amount),
            to,
        ),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Performs swap operation with the specified parameters.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **to_addr** sets the recipient of the swap operation.
pub fn swap(
    deps: DepsMut,
    sender: Addr,
    offer_asset: Asset,
    to_addr: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        offer_asset.info == config.from,
        ContractError::AssetMismatch {
            old: config.from.to_string(),
            new: config.to.to_string()
        }
    );

    let receiver = addr_opt_validate(deps.api, &to_addr)?.unwrap_or_else(|| sender.clone());

    let convert_msg = match &config.from {
        AssetInfo::Token { contract_addr } => wasm_execute(
            contract_addr,
            &cw20::Cw20ExecuteMsg::Send {
                contract: config.converter_contract.to_string(),
                amount: offer_asset.amount,
                msg: to_json_binary(&astro_converter::Cw20HookMsg {
                    receiver: Some(receiver.to_string()),
                })?,
            },
            vec![],
        )?,
        AssetInfo::NativeToken { denom } => wasm_execute(
            &config.converter_contract,
            &astro_converter::ExecuteMsg::Convert {
                receiver: Some(receiver.to_string()),
            },
            coins(offer_asset.amount.u128(), denom),
        )?,
    };

    Ok(Response::new().add_message(convert_msg).add_attributes([
        attr("action", "swap"),
        attr("receiver", receiver),
        attr("offer_asset", config.from.to_string()),
        attr("ask_asset", config.to.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", offer_asset.amount),
        attr("spread_amount", "0"),
        attr("commission_amount", "0"),
        attr("maker_fee_amount", "0"),
        attr("fee_share_amount", "0"),
    ]))
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    // phoenix-1: v1.0.1
    // pisco-1, injective-1, neutron-1: v1.3.3
    // injective-888: v1.1.0
    // pion-1: v1.3.0
    match (
        contract_version.contract.as_ref(),
        contract_version.version.as_ref(),
    ) {
        ("astroport-pair", "1.0.1" | "1.1.0" | "1.3.0" | "1.3.3") => {
            let converter_addr = deps.api.addr_validate(&msg.converter_contract)?;
            let converter_config = deps.querier.query_wasm_smart::<astro_converter::Config>(
                &converter_addr,
                &astro_converter::QueryMsg::Config {},
            )?;
            let config = migrate_config(deps.storage, converter_addr, &converter_config)?;
            sanity_checks(&config, &converter_config)?;
        }
        _ => {
            return Err(ContractError::MigrationError {
                expected: "astroport-pair:1.0.1|1.1.0|1.3.0|1.3.3".to_string(),
                current: format!("{}:{}", contract_version.contract, contract_version.version),
            })
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default().add_attributes([
        ("previous_contract_name", contract_version.contract.as_str()),
        (
            "previous_contract_version",
            contract_version.version.as_str(),
        ),
        ("new_contract_name", CONTRACT_NAME),
        ("new_contract_version", CONTRACT_VERSION),
    ]))
}
