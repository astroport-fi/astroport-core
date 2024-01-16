#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, ensure, from_json, wasm_execute, wasm_instantiate, Addr, DepsMut, Env, MessageInfo,
    Reply, Response, StdError, StdResult, SubMsg, SubMsgResponse, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use cw_utils::parse_instantiate_response_data;
use itertools::Itertools;

use astroport::asset::{
    addr_opt_validate, format_lp_token_name, Asset, AssetInfo, CoinsExt, PairInfo,
};
use astroport::factory::PairType;
use astroport::pair::{Cw20HookMsg, ExecuteMsg, InstantiateMsg};
use astroport::token::MinterResponse;

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use crate::utils::{
    assert_and_swap, check_asset_infos, check_assets, get_share_in_assets, pool_info,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID of sub-message.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(
        msg.asset_infos.len() > 1,
        ContractError::InvalidAssetLength {}
    );
    check_asset_infos(deps.api, &msg.asset_infos)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Custom("transmuter".to_string()),
        },
        factory_addr,
    };
    CONFIG.save(deps.storage, &config)?;

    let token_name = format_lp_token_name(&msg.asset_infos, &deps.querier)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        wasm_instantiate(
            msg.token_code_id,
            &astroport::token::InstantiateMsg {
                name: token_name,
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            vec![],
            String::from("Astroport LP token"),
        )?,
        INSTANTIATE_TOKEN_REPLY_ID,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: INSTANTIATE_TOKEN_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let mut config = CONFIG.load(deps.storage)?;

            if !config.pair_info.liquidity_token.as_str().is_empty() {
                return Err(
                    StdError::generic_err("Liquidity token is already set in the config").into(),
                );
            }

            let init_response = parse_instantiate_response_data(data.as_slice())?;
            config.pair_info.liquidity_token = Addr::unchecked(init_response.contract_address);
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::new()
                .add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(cw20msg) => receive_cw20(deps, info, cw20msg),
        ExecuteMsg::ProvideLiquidity {
            assets, receiver, ..
        } => provide_liquidity(deps, info, assets, receiver),
        ExecuteMsg::Swap {
            offer_asset,
            to,
            ask_asset_info,
            ..
        } => swap(deps, info, offer_asset, ask_asset_info, to),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 receive message to process.
pub fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::WithdrawLiquidity { assets } => withdraw_liquidity(
            deps,
            info,
            Addr::unchecked(cw20_msg.sender),
            cw20_msg.amount,
            assets,
        ),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Withdraw liquidity from the pool.
/// This function will burn the LP tokens and send back the assets in proportion to the withdrawn.
/// All unused LP tokens will be sent back to the sender.
///
/// * **sender** is the address that will receive assets back from the pair contract.
///
/// * **burn_amount** is the amount of LP tokens to burn.
///
/// * **assets** is the vector of assets to withdraw. If this vector is empty, the function will withdraw balanced respective to share.
pub fn withdraw_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    sender: Addr,
    mut burn_amount: Uint128,
    assets: Vec<Asset>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let (pools, total_share) = pool_info(deps.querier, &config)?;

    let mut messages = vec![];

    let refund_assets = if assets.is_empty() {
        // Usual withdraw (balanced)
        get_share_in_assets(&pools, burn_amount, total_share)?
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

                    Ok(acc + asset.amount)
                })?;

        let unused =
            burn_amount
                .checked_sub(required)
                .map_err(|_| ContractError::InsufficientLpTokens {
                    required,
                    available: burn_amount,
                })?;

        if !unused.is_zero() {
            messages.push(
                wasm_execute(
                    &config.pair_info.liquidity_token,
                    &cw20::Cw20ExecuteMsg::Transfer {
                        recipient: sender.to_string(),
                        amount: unused,
                    },
                    vec![],
                )?
                .into(),
            );
        }

        burn_amount = required;

        assets
    };

    let send_msgs = refund_assets
        .clone()
        .into_iter()
        .map(|asset| asset.into_msg(&sender))
        .collect::<StdResult<Vec<_>>>()?;
    messages.extend(send_msgs);
    messages.push(
        wasm_execute(
            &config.pair_info.liquidity_token,
            &cw20::Cw20ExecuteMsg::Burn {
                amount: burn_amount,
            },
            vec![],
        )?
        .into(),
    );

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("withdrawn_share", burn_amount),
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
        .fold(Uint128::zero(), |acc, asset| acc + asset.amount);

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    let mint_msg = wasm_execute(
        &config.pair_info.liquidity_token,
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: share,
        },
        vec![],
    )?;

    Ok(Response::new().add_message(mint_msg).add_attributes([
        attr("action", "provide_liquidity"),
        attr("receiver", receiver),
        attr("assets", assets.iter().join(", ")),
        attr("share", share),
    ]))
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

    let (return_asset, ask_asset_info) =
        assert_and_swap(deps.as_ref(), &offer_asset, ask_asset_info)?;

    let receiver = addr_opt_validate(deps.api, &to)?.unwrap_or_else(|| info.sender.clone());

    let attrs = [
        attr("action", "swap"),
        attr("receiver", &receiver),
        attr("offer_asset", offer_asset.info.to_string()),
        attr("ask_asset", ask_asset_info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_asset.amount),
        attr("spread_amount", "0"),
        attr("commission_amount", "0"),
        attr("maker_fee_amount", "0"),
    ];

    let send_msg = return_asset.into_msg(&receiver)?;

    Ok(Response::new().add_message(send_msg).add_attributes(attrs))
}
