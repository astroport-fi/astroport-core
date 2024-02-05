use cosmwasm_std::{entry_point, Decimal256, DepsMut, Env, Response, StdResult};
use injective_cosmwasm::{
    create_deposit_msg, create_withdraw_msg, InjectiveMsgWrapper, InjectiveQuerier,
    InjectiveQueryWrapper,
};

use itertools::Itertools;
use std::cmp::Ordering;

use astroport::asset::{AssetInfoExt, Decimal256Ext};

use crate::orderbook::error::OrderbookError;
use crate::orderbook::msg::SudoMsg;
use crate::orderbook::state::OrderbookState;
use crate::orderbook::utils::{
    cancel_all_orders, compute_swap, get_subaccount_balances, leave_orderbook,
    process_cumulative_trade, update_spot_orders, SpotOrdersFactory,
};
use crate::state::CONFIG;
use crate::utils::query_pools;
use astroport_pcl_common::calc_d;
use astroport_pcl_common::state::Precisions;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    msg: SudoMsg,
) -> Result<Response<InjectiveMsgWrapper>, OrderbookError> {
    match msg {
        SudoMsg::BeginBlocker {} => begin_blocker(deps, env),
        SudoMsg::Deactivate {} | SudoMsg::Deregister {} => deactivate_orderbook(deps, env),
    }
}

fn begin_blocker(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
) -> Result<Response<InjectiveMsgWrapper>, OrderbookError> {
    let ob_state = OrderbookState::load(deps.storage)?;
    if !(ob_state.enabled) {
        return Ok(Response::new());
    }
    let querier = InjectiveQuerier::new(&deps.querier);

    let balances = get_subaccount_balances(&ob_state.asset_infos, &querier, &ob_state.subaccount)?;

    if ob_state.need_reconcile || ob_state.last_balances != balances {
        let mut messages = vec![];

        let mut config = CONFIG.load(deps.storage)?;
        let precisions = Precisions::new(deps.storage)?;
        let mut pools = query_pools(
            deps.querier,
            &env.contract.address,
            &config,
            &ob_state,
            &precisions,
            Some(&balances),
        )?
        .iter()
        .map(|asset| asset.amount)
        .collect_vec();

        // Calculate total order size using the sum of an arithmetic progression
        let total_order_size = (1 + ob_state.orders_number) * ob_state.orders_number / 2;
        let liquidity_percent = Decimal256::from(ob_state.liquidity_percent);

        let base_order_size = pools[0] * liquidity_percent;
        let quote_order_size = pools[1] * liquidity_percent;

        let balances_are_zero = balances.iter().all(|balance| balance.amount.is_zero());

        // Since there is not balance before and don't meet min order size, we don't need to do anything
        if balances_are_zero
            && (base_order_size < Decimal256::from_integer(ob_state.min_base_order_size)
                || quote_order_size < Decimal256::from_integer(ob_state.min_quote_order_size))
        {
            return Ok(Response::new());
        }

        let base_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[0])?;
        let quote_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[1])?;

        // If subaccount balances have changed, then trades have occurred
        // and we need to repeg and reconcile orderbook
        if ob_state.last_balances != balances {
            let maker_fee_message = process_cumulative_trade(
                deps.querier,
                &env,
                &ob_state,
                &mut config,
                &mut pools,
                &balances,
                base_asset_precision,
                quote_asset_precision,
            )?;
            messages.extend(maker_fee_message);

            CONFIG.save(deps.storage, &config)?;
        }

        // If we previously had balances but we don't meet min order size, we leave orderbook
        if base_order_size < Decimal256::from_integer(ob_state.min_base_order_size)
            || quote_order_size < Decimal256::from_integer(ob_state.min_quote_order_size)
        {
            return leave_orderbook(&ob_state, balances, &env);
        }

        let base_trade_size =
            (base_order_size / Decimal256::from_integer(total_order_size)).floor();
        let quote_trade_size =
            (quote_order_size / Decimal256::from_integer(total_order_size)).floor();

        let amp_gamma = config.pool_state.get_amp_gamma(&env);
        let mut ixs = pools.to_vec();
        ixs[1] *= config.pool_state.price_state.price_scale;
        let d = calc_d(&ixs, &amp_gamma)?;

        let mut orders_factory = SpotOrdersFactory::new(
            &ob_state.market_id,
            &ob_state.subaccount,
            ob_state.min_price_tick_size,
            base_asset_precision,
            quote_asset_precision,
        );

        // Equal heights algorithm
        for i in 1..=ob_state.orders_number {
            let quote_sell_amount = quote_trade_size * Decimal256::from_ratio(i, 1u8);
            let base_sell_amount = compute_swap(&ixs, quote_sell_amount, 0, &config, amp_gamma, d)?;

            let sell_amount = (base_sell_amount * Decimal256::from_ratio(1u8, i)
                / ob_state.min_quantity_tick_size)
                .floor()
                * ob_state.min_quantity_tick_size;

            let sell_price = if i > 1 {
                (quote_sell_amount - orders_factory.orderbook_one_side_liquidity(false))
                    / sell_amount
            } else {
                quote_sell_amount / sell_amount
            };

            let buy_amount = base_trade_size;
            let base_buy_amount = base_trade_size * Decimal256::from_ratio(i, 1u8);
            let quote_buy_amount = compute_swap(&ixs, base_buy_amount, 1, &config, amp_gamma, d)?;
            let buy_price = if i > 1 {
                (quote_buy_amount - orders_factory.orderbook_one_side_liquidity(true)) / buy_amount
            } else {
                quote_buy_amount / base_buy_amount
            };

            // If price is zero we cancel all orders and withdraw liquidity.
            if sell_price.is_zero() || buy_price.is_zero() {
                return leave_orderbook(&ob_state, balances, &env);
            }

            orders_factory.sell(sell_price, sell_amount);
            orders_factory.buy(buy_price, buy_amount);
        }

        let total_deposits =
            orders_factory.total_deposit(&config.pair_info.asset_infos, &precisions)?;

        // Cancel all orders first
        messages.push(cancel_all_orders(
            &env.contract.address,
            &ob_state.subaccount,
            &ob_state.market_id,
        ));

        // Adjust subaccount balances
        total_deposits
            .iter()
            .zip(balances.iter())
            .try_for_each::<_, StdResult<_>>(|(need, current)| {
                match need.amount.cmp(&current.amount) {
                    Ordering::Greater => messages.push(create_deposit_msg(
                        env.contract.address.clone(),
                        ob_state.subaccount.clone(),
                        need.info
                            .with_balance(need.amount - current.amount)
                            .as_coin()?,
                    )),
                    Ordering::Less => {
                        messages.push(create_withdraw_msg(
                            env.contract.address.clone(),
                            ob_state.subaccount.clone(),
                            need.info
                                .with_balance(current.amount - need.amount)
                                .as_coin()?,
                        ));
                    }
                    Ordering::Equal => {}
                }

                Ok(())
            })?;

        let new_orders = orders_factory.collect_orders(&env.contract.address)?;
        messages.push(update_spot_orders(&env.contract.address, new_orders));

        ob_state.reconciliation_done(deps.storage, total_deposits)?;

        Ok(Response::new().add_messages(messages))
    } else {
        Ok(Response::default())
    }
}

/// This function is called when chain for some reason wants to remove our contract from begin blocker.
/// The reasons I know at the moment are:
/// - contract does not have enough INJ balance to pay gas fees
/// - governance decided to remove contract from begin blocker
///
/// In that case we disable orderbook integration, cancel all orders and withdraw all subaccount balances.
/// This function may fail due to out of gas error thus for safety reasons we have permissionless endpoint
/// [`astroport::pair_concentrated_inj::ExecuteMsg::WithdrawFromOrderbook`] to perform the same action.
fn deactivate_orderbook(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
) -> Result<Response<InjectiveMsgWrapper>, OrderbookError> {
    deps.api.debug(&format!(
        "Deactivating Astroport pair {} orderbook integration",
        &env.contract.address
    ));
    let mut ob_state = OrderbookState::load(deps.storage)?;
    ob_state.enabled = false;
    ob_state.last_balances = vec![
        ob_state.asset_infos[0].with_balance(0u8),
        ob_state.asset_infos[1].with_balance(0u8),
    ];
    ob_state.save(deps.storage)?;

    let querier = InjectiveQuerier::new(&deps.querier);
    let balances = get_subaccount_balances(&ob_state.asset_infos, &querier, &ob_state.subaccount)?;

    Ok(leave_orderbook(&ob_state, balances, &env)?
        .add_attribute("action", "deactivate")
        .add_attribute("pair", &env.contract.address))
}
