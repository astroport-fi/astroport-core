#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure, ensure_eq, from_json, Addr, Binary, Decimal, Decimal256, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw_utils::must_pay;
use itertools::Itertools;

use astroport::asset::{
    addr_opt_validate, Asset, AssetInfo, AssetInfoExt, CoinsExt, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::cosmwasm_ext::{DecimalToInteger, IntegerToDecimal};
use astroport::pair::{ExecuteMsgExt, FeeShareConfig, MAX_FEE_SHARE_BPS, MIN_TRADE_SIZE};
use astroport::pair_concentrated::ConcentratedPoolUpdateParams;
use astroport::pair_concentrated_duality::DualityPairMsg;
use astroport::querier::{query_factory_config, query_fee_info, query_native_supply};
use astroport::token_factory::tf_burn_msg;
use astroport_pcl_common::state::Precisions;
use astroport_pcl_common::utils::{
    accumulate_prices, assert_max_spread, before_swap_check, calc_last_prices, compute_swap,
    get_share_in_assets, mint_liquidity_token_message,
};
use astroport_pcl_common::{calc_d, get_xcp};

use crate::error::ContractError;
use crate::instantiate::LP_TOKEN_PRECISION;
use crate::orderbook::execute::{process_cumulative_trade, sync_pool_with_orderbook};
use crate::orderbook::state::OrderbookState;
use crate::orderbook::utils::{fetch_cumulative_trade, Liquidity};
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL};
use crate::utils::{calculate_shares, ensure_min_assets_to_receive, get_assets_with_precision};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsgExt<DualityPairMsg>,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsgExt::ProvideLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        ),
        ExecuteMsgExt::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
            ..
        } => {
            offer_asset.assert_sent_native_token_balance(&info)?;

            let config = CONFIG.load(deps.storage)?;

            if !config.pair_info.asset_infos.contains(&offer_asset.info) {
                return Err(ContractError::InvalidAsset(offer_asset.info.to_string()));
            }

            let to_addr = addr_opt_validate(deps.api, &to)?;

            swap(
                deps,
                env,
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
        ExecuteMsgExt::WithdrawLiquidity {
            assets,
            min_assets_to_receive,
        } => withdraw_liquidity(deps, env, info, assets, min_assets_to_receive),
        ExecuteMsgExt::UpdateConfig { params } => update_config(deps, env, info, params),
        ExecuteMsgExt::Custom(duality_msg) => process_custom_msgs(deps, env, info, duality_msg),
        ExecuteMsgExt::ProposeNewOwner { owner, expires_in } => {
            let config = CONFIG.load(deps.storage)?;
            let factory_config = query_factory_config(&deps.querier, config.factory_addr)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsgExt::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;
            let factory_config = query_factory_config(&deps.querier, config.factory_addr)?;

            drop_ownership_proposal(
                deps,
                info,
                config.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsgExt::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut config| {
                        config.owner = Some(new_owner);
                        Ok(config)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        _ => unimplemented!("Unsupported message"),
    }
}

/// Provides liquidity in the pair with the specified input parameters.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
/// * **auto_stake** is an optional parameter which determines whether the LP tokens minted after
/// liquidity provision are automatically staked in the Generator contract on behalf of the LP token receiver.
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
///
#[allow(clippy::too_many_arguments)]
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    mut assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
    min_lp_to_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    let precisions = Precisions::new(deps.storage)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &ob_state, false)?;

    // This call fetches possible cumulative trade
    let maybe_cumulative_trade =
        fetch_cumulative_trade(&precisions, &ob_state.last_balances, &liquidity.orderbook)?;

    let mut pools = liquidity.total_dec(&precisions)?;

    let old_real_price = config.pool_state.price_state.last_price;

    let deposits =
        get_assets_with_precision(deps.as_ref(), &config, &mut assets, &pools, &precisions)?;

    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let mut mint_lp_messages = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        match &pool.info {
            AssetInfo::Token { .. } => unreachable!("cw20 tokens not supported"),
            AssetInfo::NativeToken { .. } => {
                // If the asset is native token, the pool balance is already increased
                // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
                pool.amount = pool.amount.checked_sub(deposits[i])?;
            }
        }
    }

    // Process all filled orders as one cumulative trade; send maker fees; repeg PCL
    let response = if let Some(cumulative_trade) = maybe_cumulative_trade {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trade(
            deps.as_ref(),
            &env,
            &cumulative_trade,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?
    } else {
        Response::default()
    };

    let (share_uint128, slippage) = calculate_shares(
        &env,
        &mut config,
        &pools,
        total_share,
        &deposits,
        slippage_tolerance,
    )?;

    if total_share.is_zero() {
        mint_lp_messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);
    }

    let min_amount_lp = min_lp_to_receive.unwrap_or_default();
    ensure!(
        share_uint128 >= min_amount_lp,
        ContractError::ProvideSlippageViolation(share_uint128, min_amount_lp)
    );

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    let auto_stake = auto_stake.unwrap_or(false);
    mint_lp_messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        share_uint128,
        auto_stake,
    )?);

    accumulate_prices(&env, &mut config, old_real_price);

    // Reconcile orders
    // Adding deposits back to the pool balances
    let balances = pools
        .iter()
        .zip(deposits.iter())
        .map(|(asset, deposit)| asset.amount + deposit)
        .collect_vec();
    let cancel_msgs = ob_state.cancel_orders(&env.contract.address);
    let order_msgs = ob_state.deploy_orders(&env, &config, &balances, &precisions)?;

    CONFIG.save(deps.storage, &config)?;

    let pools_u128 = pools
        .iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info).unwrap();
            let amount = asset.amount.to_uint(prec)?;
            Ok(asset.info.with_balance(amount))
        })
        .collect::<StdResult<Vec<_>>>()?;
    let submsgs = ob_state.flatten_msgs_and_add_callback(
        &pools_u128,
        &[cancel_msgs, mint_lp_messages],
        order_msgs,
    );
    ob_state.save(deps.storage)?;

    Ok(response.add_submessages(submsgs).add_attributes([
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", format!("{}, {}", &assets[0], &assets[1])),
        attr("share", share_uint128),
        attr("slippage", slippage.to_string()),
    ]))
}

/// Withdraw liquidity from the pool.
///
/// * **sender** address that will receive assets back from the pair contract
///
/// * **amount** amount of provided LP tokens
///
/// * **assets** defines number of coins a user wants to withdraw per each asset.
fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let amount = must_pay(&info, &config.pair_info.liquidity_token)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let precisions = Precisions::new(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &ob_state, false)?;

    // This call fetches possible cumulative trade
    let maybe_cumulative_trade =
        fetch_cumulative_trade(&precisions, &ob_state.last_balances, &liquidity.orderbook)?;

    let mut pools = liquidity.total_dec(&precisions)?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;

    // Process all filled orders as one cumulative trade; send maker fees; repeg PCL
    let response = if let Some(cumulative_trade) = maybe_cumulative_trade {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trade(
            deps.as_ref(),
            &env,
            &cumulative_trade,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?
    } else {
        Response::default()
    };

    // In any case, we cancel all orders
    let cancel_msgs = ob_state.cancel_orders(&env.contract.address);

    let refund_assets = if assets.is_empty() {
        // Usual withdraw (balanced)
        get_share_in_assets(&pools, amount.saturating_sub(Uint128::one()), total_share)
    } else {
        return Err(StdError::generic_err("Imbalanced withdraw is currently disabled").into());
    };

    let mut xs = pools.iter().map(|a| a.amount).collect_vec();

    // Reflect balance changes after withdrawal
    xs[0] -= refund_assets[0].amount;
    xs[1] -= refund_assets[1].amount;

    // decrease XCP
    xs[1] *= config.pool_state.price_state.price_scale;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let d = calc_d(&xs, &amp_gamma)?;
    config.pool_state.price_state.xcp_profit_real =
        get_xcp(d, config.pool_state.price_state.price_scale)
            / (total_share - amount).to_decimal256(LP_TOKEN_PRECISION)?;

    let mut pools_u128 = pools
        .iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info).unwrap();
            let amount = asset.amount.to_uint(prec)?;
            Ok(asset.info.with_balance(amount))
        })
        .collect::<StdResult<Vec<_>>>()?;

    let refund_assets = refund_assets
        .into_iter()
        .enumerate()
        .map(|(ind, asset)| {
            let prec = precisions.get_precision(&asset.info).unwrap();
            let amount = asset.amount.to_uint(prec)?;

            pools_u128[ind].amount -= amount;

            Ok(asset.info.with_balance(amount))
        })
        .collect::<StdResult<Vec<_>>>()?;

    ensure_min_assets_to_receive(&config, &refund_assets, min_assets_to_receive)?;

    let mut withdraw_messages = refund_assets
        .iter()
        .cloned()
        .map(|asset| asset.into_msg(&info.sender))
        .collect::<StdResult<Vec<_>>>()?;
    withdraw_messages.push(tf_burn_msg(
        &env.contract.address,
        coin(amount.u128(), &config.pair_info.liquidity_token),
    ));

    CONFIG.save(deps.storage, &config)?;

    let order_msgs = ob_state.deploy_orders(&env, &config, &xs, &precisions)?;
    let submsgs = ob_state.flatten_msgs_and_add_callback(
        &pools_u128,
        &[cancel_msgs, withdraw_messages],
        order_msgs,
    );
    ob_state.save(deps.storage)?;

    Ok(response.add_submessages(submsgs).add_attributes([
        attr("action", "withdraw_liquidity"),
        attr("sender", info.sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}

/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **belief_price** is used to calculate the maximum swap spread.
///
/// * **max_spread** sets the maximum spread of the swap operation.
///
/// * **to** sets the recipient of the swap operation.
fn swap(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;
    let mut config = CONFIG.load(deps.storage)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &ob_state, false)?;

    // This call fetches possible cumulative trade
    let maybe_cumulative_trade =
        fetch_cumulative_trade(&precisions, &ob_state.last_balances, &liquidity.orderbook)?;

    let mut pools = liquidity.total_dec(&precisions)?;

    let (offer_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == offer_asset_dec.info)
        .ok_or_else(|| ContractError::InvalidAsset(offer_asset_dec.info.to_string()))?;
    let ask_ind = 1 ^ offer_ind;
    let ask_asset_prec = precisions.get_precision(&pools[ask_ind].info)?;

    pools[offer_ind].amount -= offer_asset_dec.amount;
    before_swap_check(&pools, offer_asset_dec.amount)?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    // Process all filled orders as one cumulative trade; send maker fees; repeg PCL
    let response = if let Some(cumulative_trade) = maybe_cumulative_trade {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trade(
            deps.as_ref(),
            &env,
            &cumulative_trade,
            &mut config,
            &mut balances,
            &precisions,
            Some(&fee_info),
        )?
    } else {
        Response::default()
    };

    let mut xs = pools.iter().map(|asset| asset.amount).collect_vec();
    let old_real_price = calc_last_prices(&xs, &config, &env)?;

    let mut maker_fee_share = Decimal256::zero();
    if fee_info.fee_address.is_some() {
        maker_fee_share = fee_info.maker_fee_rate.into();
    }
    // If this pool is configured to share fees
    let mut share_fee_share = Decimal256::zero();
    if let Some(fee_share) = &config.fee_share {
        share_fee_share = Decimal256::from_ratio(fee_share.bps, 10000u16);
    }

    let swap_result = compute_swap(
        &xs,
        offer_asset_dec.amount,
        ask_ind,
        &config,
        &env,
        maker_fee_share,
        share_fee_share,
    )?;
    xs[offer_ind] += offer_asset_dec.amount;
    xs[ask_ind] -= swap_result.dy + swap_result.maker_fee + swap_result.share_fee;

    let return_amount = swap_result.dy.to_uint(ask_asset_prec)?;
    let spread_amount = swap_result.spread_fee.to_uint(ask_asset_prec)?;
    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset.amount,
        return_amount,
        spread_amount,
    )?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    // Skip very small trade sizes which could significantly mess up the price due to rounding errors,
    // especially if token precisions are 18.
    if (swap_result.dy + swap_result.maker_fee + swap_result.share_fee) >= MIN_TRADE_SIZE
        && offer_asset_dec.amount >= MIN_TRADE_SIZE
    {
        let last_price = swap_result.calc_last_price(offer_asset_dec.amount, offer_ind);

        // update_price() works only with internal representation
        let ixs = [xs[0], xs[1] * config.pool_state.price_state.price_scale];
        config
            .pool_state
            .update_price(&config.pool_params, &env, total_share, &ixs, last_price)?;
    }

    let receiver = to.unwrap_or_else(|| sender.clone());

    let mut messages = vec![pools[ask_ind]
        .info
        .with_balance(return_amount)
        .into_msg(&receiver)?];

    let mut pools_u128 = pools
        .iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info).unwrap();
            let amount = asset.amount.to_uint(prec)?;
            Ok(asset.info.with_balance(amount))
        })
        .collect::<StdResult<Vec<_>>>()?;
    pools_u128[offer_ind].amount += offer_asset.amount;
    pools_u128[ask_ind].amount -= return_amount;

    // Send the shared fee
    let mut fee_share_amount = Uint128::zero();
    if let Some(fee_share) = &config.fee_share {
        fee_share_amount = swap_result.share_fee.to_uint(ask_asset_prec)?;
        if !fee_share_amount.is_zero() {
            let fee = pools[ask_ind].info.with_balance(fee_share_amount);
            messages.push(fee.into_msg(&fee_share.recipient)?);
            pools_u128[ask_ind].amount -= fee_share_amount;
        }
    }

    // Send the maker fee
    let mut maker_fee = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        maker_fee = swap_result.maker_fee.to_uint(ask_asset_prec)?;
        if !maker_fee.is_zero() {
            let fee = pools[ask_ind].info.with_balance(maker_fee);
            messages.push(fee.into_msg(fee_address)?);
            pools_u128[ask_ind].amount -= maker_fee;
        }
    }

    accumulate_prices(&env, &mut config, old_real_price);

    // Reconcile orders
    let cancel_msgs = ob_state.cancel_orders(&env.contract.address);
    let order_msgs = ob_state.deploy_orders(&env, &config, &xs, &precisions)?;

    CONFIG.save(deps.storage, &config)?;

    let submsgs =
        ob_state.flatten_msgs_and_add_callback(&pools_u128, &[cancel_msgs, messages], order_msgs);

    ob_state.save(deps.storage)?;

    Ok(response.add_submessages(submsgs).add_attributes([
        attr("action", "swap"),
        attr("sender", sender),
        attr("receiver", receiver),
        attr("offer_asset", offer_asset_dec.info.to_string()),
        attr("ask_asset", pools[ask_ind].info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_amount),
        attr("spread_amount", spread_amount),
        attr(
            "commission_amount",
            swap_result.total_fee.to_uint(ask_asset_prec)?,
        ),
        attr("maker_fee_amount", maker_fee),
        attr("fee_share_amount", fee_share_amount),
    ]))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values in [`Binary`] form.
fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    let owner = config.owner.as_ref().unwrap_or(&factory_config.owner);
    ensure_eq!(&info.sender, owner, ContractError::Unauthorized {});

    let mut attrs = vec![];

    let action = match from_json(params)? {
        ConcentratedPoolUpdateParams::Update(update_params) => {
            config.pool_params.update_params(update_params)?;
            "update_params"
        }
        ConcentratedPoolUpdateParams::Promote(promote_params) => {
            config.pool_state.promote_params(&env, promote_params)?;
            "promote_params"
        }
        ConcentratedPoolUpdateParams::StopChangingAmpGamma {} => {
            config.pool_state.stop_promotion(&env);
            "stop_changing_amp_gamma"
        }
        ConcentratedPoolUpdateParams::EnableFeeShare {
            fee_share_bps,
            fee_share_address,
        } => {
            // Enable fee sharing for this contract
            // If fee sharing is already enabled, we should be able to overwrite
            // the values currently set

            // Ensure the fee share isn't 0 and doesn't exceed the maximum allowed value
            ensure!(
                (1..=MAX_FEE_SHARE_BPS).contains(&fee_share_bps),
                ContractError::FeeShareOutOfBounds {}
            );

            // Set sharing config
            config.fee_share = Some(FeeShareConfig {
                bps: fee_share_bps,
                recipient: deps.api.addr_validate(&fee_share_address)?,
            });

            CONFIG.save(deps.storage, &config)?;

            attrs.push(attr("fee_share_bps", fee_share_bps.to_string()));
            attrs.push(attr("fee_share_address", fee_share_address));
            "enable_fee_share"
        }
        ConcentratedPoolUpdateParams::DisableFeeShare => {
            // Disable fee sharing for this contract by setting bps and
            // address back to None
            config.fee_share = None;
            CONFIG.save(deps.storage, &config)?;
            "disable_fee_share"
        }
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", action)
        .add_attributes(attrs))
}

pub fn process_custom_msgs(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: DualityPairMsg,
) -> Result<Response, ContractError> {
    match msg {
        DualityPairMsg::SyncOrderbook {} => sync_pool_with_orderbook(deps, env, info),
        DualityPairMsg::UpdateOrderbookConfig(update_orderbook_conf) => {
            let config = CONFIG.load(deps.storage)?;
            let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

            let owner = config.owner.unwrap_or(factory_config.owner);
            ensure_eq!(info.sender, owner, ContractError::Unauthorized {});

            let mut ob_state = OrderbookState::load(deps.storage)?;
            let cancel_orders_msgs = if let Some(false) = update_orderbook_conf.enable {
                let msgs = ob_state.cancel_orders(&env.contract.address);
                ob_state.orders = vec![];
                msgs
            } else {
                vec![]
            };

            let mut attrs = vec![attr("action", "update_duality_orderbook_config")];
            attrs.extend(ob_state.update_config(deps.api, update_orderbook_conf)?);

            ob_state.save(deps.storage)?;

            Ok(Response::default()
                .add_messages(cancel_orders_msgs)
                .add_attributes(attrs))
        }
    }
}
