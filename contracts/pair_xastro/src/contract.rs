use cosmwasm_std::{
    attr, ensure, from_json, wasm_execute, Addr, DepsMut, Env, MessageInfo, QuerierWrapper,
    Response, StdError, Uint128,
};

use astroport::asset::{addr_opt_validate, Asset, AssetInfo, PairInfo};
use astroport::pair::ExecuteMsg;
use astroport::pair_xastro::XastroPairInitParams;
use astroport::{pair, staking};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum initial xastro share
pub(crate) const MINIMUM_STAKE_AMOUNT: Uint128 = Uint128::new(1_000);

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: pair::InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.asset_infos.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }

    let params: XastroPairInitParams = msg
        .init_params
        .map(from_json)
        .transpose()?
        .ok_or_else(|| StdError::generic_err("Missing init params"))?;

    let staking_config: staking::Config = deps
        .querier
        .query_wasm_smart(&params.staking, &staking::QueryMsg::Config {})?;

    ensure!(
        msg.asset_infos
            .contains(&AssetInfo::native(&staking_config.astro_denom)),
        StdError::generic_err("Missing astro denom in asset_infos")
    );
    ensure!(
        msg.asset_infos
            .contains(&AssetInfo::native(&staking_config.xastro_denom)),
        StdError::generic_err("Missing xAstro denom in asset_infos")
    );

    CONFIG.save(
        deps.storage,
        &Config {
            pair_info: PairInfo {
                asset_infos: msg.asset_infos,
                contract_addr: env.contract.address.clone(),
                liquidity_token: "".to_string(),
                pair_type: msg.pair_type,
            },
            factory_addr: deps.api.addr_validate(&msg.factory_addr)?,
            staking: Addr::unchecked(params.staking),
            astro_denom: staking_config.astro_denom,
            xastro_denom: staking_config.xastro_denom,
        },
    )?;

    Ok(Response::new())
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
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Swap {
            offer_asset, to, ..
        } => {
            offer_asset.assert_sent_native_token_balance(&info)?;
            swap(deps, info.sender, offer_asset, to)
        }
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

    let receiver = addr_opt_validate(deps.api, &to_addr)?.unwrap_or_else(|| sender.clone());

    match &offer_asset.info {
        AssetInfo::NativeToken { denom } if denom == &config.astro_denom => {
            let stake_msg = wasm_execute(
                &config.staking,
                &staking::ExecuteMsg::Enter {
                    receiver: Some(receiver.to_string()),
                },
                vec![offer_asset.as_coin().unwrap()],
            )?;

            let mint_amount = predict_stake(deps.querier, &config, offer_asset.amount)?;

            Ok(Response::new().add_message(stake_msg).add_attributes([
                attr("action", "swap"),
                attr("receiver", receiver),
                attr("offer_asset", &config.astro_denom),
                attr("ask_asset", &config.xastro_denom),
                attr("offer_amount", offer_asset.amount),
                attr("return_amount", mint_amount),
                attr("spread_amount", "0"),
                attr("commission_amount", "0"),
                attr("maker_fee_amount", "0"),
                attr("fee_share_amount", "0"),
            ]))
        }
        AssetInfo::NativeToken { denom } if denom == &config.xastro_denom => {
            let unstake_msg = wasm_execute(
                &config.staking,
                &staking::ExecuteMsg::Leave {
                    receiver: Some(receiver.to_string()),
                },
                vec![offer_asset.as_coin().unwrap()],
            )?;

            let return_amount = predict_unstake(deps.querier, &config, offer_asset.amount)?;

            Ok(Response::new().add_message(unstake_msg).add_attributes([
                attr("action", "swap"),
                attr("receiver", receiver),
                attr("offer_asset", &config.xastro_denom),
                attr("ask_asset", &config.astro_denom),
                attr("offer_amount", offer_asset.amount),
                attr("return_amount", return_amount),
                attr("spread_amount", "0"),
                attr("commission_amount", "0"),
                attr("maker_fee_amount", "0"),
                attr("fee_share_amount", "0"),
            ]))
        }
        _ => Err(ContractError::InvalidAsset(offer_asset.info.to_string())),
    }
}

pub fn query_deposit_and_shares(
    querier: QuerierWrapper,
    config: &Config,
) -> Result<(Uint128, Uint128), ContractError> {
    let total_deposit = querier
        .query_balance(&config.staking, &config.astro_denom)?
        .amount;
    let total_shares = querier.query_supply(&config.xastro_denom)?.amount;

    Ok((total_deposit, total_shares))
}

pub fn predict_stake(
    querier: QuerierWrapper,
    config: &Config,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    let (total_deposit, total_shares) = query_deposit_and_shares(querier, config)?;

    if total_deposit.is_zero() {
        if amount.saturating_sub(MINIMUM_STAKE_AMOUNT).is_zero() {
            return Err(ContractError::MinimumStakeAmountError {});
        }

        Ok(amount - MINIMUM_STAKE_AMOUNT)
    } else {
        Ok(amount.multiply_ratio(total_shares, total_deposit))
    }
}

pub fn predict_unstake(
    querier: QuerierWrapper,
    config: &Config,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    let (total_deposit, total_shares) = query_deposit_and_shares(querier, config)?;

    ensure!(
        total_shares >= amount,
        ContractError::InvalidUnstakeAmount {
            want: amount,
            total: total_shares
        }
    );

    Ok(amount.multiply_ratio(total_deposit, total_shares))
}
