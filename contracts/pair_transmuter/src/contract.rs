use astroport::token_factory::{
    tf_burn_msg, tf_create_denom_msg, tf_mint_msg, MsgCreateDenomResponse,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure, ensure_eq, BankMsg, Coin, DepsMut, Empty, Env, Event, MessageInfo, Reply,
    Response, StdError, StdResult, SubMsg, SubMsgResponse, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use cw_utils::{one_coin, PaymentError};
use itertools::Itertools;

use astroport::asset::{addr_opt_validate, Asset, AssetInfo, CoinsExt, PairInfo};
use astroport::factory::PairType;
use astroport::pair::{ExecuteMsg, InstantiateMsg};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use crate::utils::{
    assert_and_swap, check_asset_infos, check_assets, get_share_in_assets, pool_info,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Reply ID for create denom reply
const CREATE_DENOM_REPLY_ID: u64 = 1;
/// Tokenfactory LP token subdenom
pub const LP_SUBDENOM: &str = "astroport/share";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(
        msg.asset_infos.len() > 1 && msg.asset_infos.len() <= 5,
        ContractError::InvalidAssetLength {}
    );
    check_asset_infos(deps.api, &msg.asset_infos)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;

    let pair_info = PairInfo {
        contract_addr: env.contract.address.clone(),
        liquidity_token: "".to_owned(),
        asset_infos: msg.asset_infos.clone(),
        pair_type: PairType::Custom("transmuter".to_string()),
    };

    let config = Config::new(deps.querier, pair_info, factory_addr)?;

    CONFIG.save(deps.storage, &config)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        tf_create_denom_msg(env.contract.address.to_string(), LP_SUBDENOM),
        CREATE_DENOM_REPLY_ID,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: CREATE_DENOM_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let MsgCreateDenomResponse { new_token_denom } = data.try_into()?;

            CONFIG.update(deps.storage, |mut config| {
                if !config.pair_info.liquidity_token.is_empty() {
                    return Err(ContractError::Unauthorized {});
                }

                config.pair_info.liquidity_token = new_token_denom.clone();
                Ok(config)
            })?;

            Ok(Response::new().add_attribute("lp_denom", new_token_denom))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ProvideLiquidity {
            assets,
            auto_stake,
            receiver,
            ..
        } => {
            ensure!(
                auto_stake.is_none() || matches!(auto_stake, Some(false)),
                StdError::generic_err("Auto stake is not supported")
            );

            provide_liquidity(deps, env, info, assets, receiver)
        }
        ExecuteMsg::Swap {
            offer_asset,
            to,
            ask_asset_info,
            ..
        } => swap(deps, info, offer_asset, ask_asset_info, to),
        ExecuteMsg::WithdrawLiquidity { assets, .. } => withdraw_liquidity(deps, env, info, assets),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Withdraw liquidity from the pool.
/// This function will burn the LP tokens and send back the assets in proportion to the withdrawn.
/// All unused LP tokens will be sent back to the sender.
///
/// * **assets** is the vector of assets to withdraw. If this vector is empty, the function will withdraw balanced respective to share.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let Coin { mut amount, denom } = one_coin(&info)?;

    ensure_eq!(
        denom,
        config.pair_info.liquidity_token,
        PaymentError::MissingDenom(config.pair_info.liquidity_token.to_string())
    );
    let (pools, total_share) = pool_info(deps.querier, &config)?;

    let mut messages = vec![];

    let refund_assets = if assets.is_empty() {
        // Usual withdraw (balanced)
        get_share_in_assets(&pools, amount, total_share)?
    } else {
        let required =
            assets
                .iter()
                .try_fold(Uint128::zero(), |acc, asset| -> Result<_, ContractError> {
                    let pool = pools
                        .iter()
                        .find(|p| p.info == asset.info)
                        .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

                    pool.amount.checked_sub(asset.amount).map_err(|_| {
                        ContractError::InsufficientPoolBalance {
                            asset: asset.info.to_string(),
                            want: asset.amount,
                            available: pool.amount,
                        }
                    })?;

                    let normalized_asset = config.normalize(asset)?;

                    Ok(acc + normalized_asset.amount)
                })?;

        let unused =
            amount
                .checked_sub(required)
                .map_err(|_| ContractError::InsufficientLpTokens {
                    required,
                    available: amount,
                })?;

        if !unused.is_zero() {
            messages.push(
                BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: vec![coin(unused.u128(), &config.pair_info.liquidity_token)],
                }
                .into(),
            );
        }

        amount = required;

        assets
    };

    let send_msgs = refund_assets
        .clone()
        .into_iter()
        .filter(|asset| !asset.amount.is_zero())
        .map(|asset| asset.into_msg(&info.sender))
        .collect::<StdResult<Vec<_>>>()?;
    messages.extend(send_msgs);
    messages.push(tf_burn_msg(
        env.contract.address,
        coin(amount.u128(), config.pair_info.liquidity_token.to_string()),
        info.sender.to_string(),
    ));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}

/// Provides liquidity with the specified input parameters.
///
/// * **assets** vector with assets meant to be provided to the pool.
///
/// * **receiver** address that receives LP tokens. If this address isn't specified, the function will default to the caller.
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    check_assets(deps.api, &assets)?;

    let config = CONFIG.load(deps.storage)?;
    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    // Share is simply sum of all provided assets because this pool maintains 1:1 ratio
    let share = assets
        .iter()
        .map(|asset| config.normalize(asset).unwrap())
        .fold(Uint128::zero(), |acc, asset| acc + asset.amount);

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());

    Ok(Response::new()
        .add_message(tf_mint_msg(
            env.contract.address,
            coin(share.into(), config.pair_info.liquidity_token.to_string()),
            &receiver,
        ))
        .add_event(
            Event::new("astroport-pool.v1.ProvideLiqudity").add_attributes([
                attr("action", "provide_liquidity"),
                attr("receiver", &receiver),
                attr("assets", assets.iter().join(", ")),
                attr("share", share),
            ]),
        )
        .add_event(Event::new("astroport-pool.v1.Mint").add_attributes([
            attr("action", "mint"),
            attr("to", receiver),
            attr("amount", share),
        ])))
}

/// Performs an swap operation with the specified parameters.
/// * **offer_asset** proposed asset for swapping.
///
/// * **ask_asset_info** is the asset to be received after the swap operation.
/// Must be set if the pool contains more than 2 assets.
///
/// * **to** sets the recipient of the swap operation.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    offer_asset: Asset,
    ask_asset_info: Option<AssetInfo>,
    to: Option<String>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;

    let return_asset = assert_and_swap(deps.as_ref(), &offer_asset, ask_asset_info)?;

    let receiver = addr_opt_validate(deps.api, &to)?.unwrap_or_else(|| info.sender.clone());

    let attrs = [
        attr("action", "swap"),
        attr("receiver", &receiver),
        attr("offer_asset", offer_asset.info.to_string()),
        attr("ask_asset", return_asset.info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_asset.amount),
        attr("spread_amount", "0"),
        attr("commission_amount", "0"),
        attr("maker_fee_amount", "0"),
    ];

    let send_msg = return_asset.into_msg(&receiver)?;

    Ok(Response::new().add_message(send_msg).add_attributes(attrs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    let contract_version = cw2::get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-pair-transmuter" => match contract_version.version.as_ref() {
            "1.1.0" => {}
            _ => {
                return Err(StdError::generic_err(
                    "Cannot migrate. Unsupported contract version",
                ))
            }
        },
        _ => {
            return Err(StdError::generic_err(
                "Cannot migrate. Unsupported contract name",
            ))
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
