use std::cmp::Ordering;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure_eq, to_json_string, Decimal256, Deps, DepsMut, Env, Event, MessageInfo, Response,
    StdResult,
};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfoExt, DecimalAsset};
use astroport::cosmwasm_ext::{DecimalToInteger, IntegerToDecimal};
use astroport::pair::MIN_TRADE_SIZE;
use astroport::querier::{query_fee_info, query_native_supply, FeeInfo};
use astroport_pcl_common::state::{Config, Precisions};
use astroport_pcl_common::utils::{accumulate_prices, calc_last_prices};

use crate::error::ContractError;
use crate::instantiate::LP_TOKEN_PRECISION;
use crate::orderbook::utils::Liquidity;
use crate::state::CONFIG;

use super::error::OrderbookError;
use super::state::OrderbookState;

/// CumulativeTrade represents all trades on one side that happened on orderbook as one trade.
/// I.e., swap from base_asset -> quote_asset.
/// In this context, Astroport always charges protocol fees from quote asset.
#[cw_serde]
pub struct CumulativeTrade {
    /// An asset that was sold
    pub base_asset: DecimalAsset,
    /// An asset that was bought
    pub quote_asset: DecimalAsset,
}

impl CumulativeTrade {
    pub fn try_into_uint(
        &self,
        precisions: &Precisions,
    ) -> Result<CumulativeTradeUint, OrderbookError> {
        let into_uint_asset = |dec_asset: &DecimalAsset| {
            let amount = dec_asset
                .amount
                .to_uint(precisions.get_precision(&dec_asset.info)?)?;
            Ok::<_, OrderbookError>(dec_asset.info.with_balance(amount))
        };
        Ok(CumulativeTradeUint {
            base_asset: into_uint_asset(&self.base_asset)?,
            quote_asset: into_uint_asset(&self.quote_asset)?,
        })
    }
}

/// Auxiliary type fo indexing purposes
#[cw_serde]
pub struct CumulativeTradeUint {
    /// An asset that was sold
    pub base_asset: Asset,
    /// An asset that was bought
    pub quote_asset: Asset,
}

/// Process fees from one or two trades (depending on whether both sell and buy sides were crossed);
/// Combine them into one trade and repeg PCL.
pub fn process_cumulative_trades(
    deps: Deps,
    env: &Env,
    trades: &[CumulativeTrade],
    config: &mut Config,
    balances: &mut [&mut Decimal256],
    precisions: &Precisions,
    fee_info: Option<&FeeInfo>,
) -> Result<Response, OrderbookError> {
    let fee_info = if let Some(fee_info) = fee_info.cloned() {
        fee_info
    } else {
        query_fee_info(
            &deps.querier,
            &config.factory_addr,
            config.pair_info.pair_type.clone(),
        )?
    };

    let mut messages = vec![];
    let mut events = vec![];

    for (i, trade) in trades.iter().enumerate() {
        let offer_ind = config
            .pair_info
            .asset_infos
            .iter()
            .position(|asset_info| asset_info == &trade.base_asset.info)
            .unwrap();
        let ask_ind = 1 ^ offer_ind;
        let ask_asset_prec = precisions.get_precision(&config.pair_info.asset_infos[ask_ind])?;

        // Using max possible fee because this was the fee used while posting orders
        let total_fee = Decimal256::from(config.pool_params.out_fee) * trade.quote_asset.amount;

        let mut attrs = vec![
            (
                "cumulative_trade",
                to_json_string(&trade.try_into_uint(precisions)?)?,
            ),
            (
                "total_fee_amount",
                total_fee.to_uint(ask_asset_prec)?.to_string(),
            ),
        ];

        let mut share_amount = Decimal256::zero();
        // Send the shared fee
        if let Some(fee_share) = &config.fee_share {
            share_amount = total_fee * Decimal256::from_ratio(fee_share.bps, 10000u16);
            *balances[ask_ind] -= share_amount;

            let fee_share_amount = share_amount.to_uint(ask_asset_prec)?;
            if !fee_share_amount.is_zero() {
                let fee = config.pair_info.asset_infos[ask_ind].with_balance(fee_share_amount);
                attrs.push(("fee_share_amount", fee_share_amount.to_string()));
                messages.push(fee.into_msg(&fee_share.recipient)?);
            }
        }

        // Send the maker fee
        if let Some(fee_address) = &fee_info.fee_address {
            let maker_share =
                (total_fee - share_amount) * Decimal256::from(fee_info.maker_fee_rate);
            *balances[ask_ind] -= maker_share;

            let maker_fee = maker_share.to_uint(ask_asset_prec)?;
            if !maker_fee.is_zero() {
                let fee = config.pair_info.asset_infos[ask_ind].with_balance(maker_fee);
                attrs.push(("maker_fee_amount", maker_fee.to_string()));
                messages.push(fee.into_msg(fee_address)?);
            }
        }

        events.push(Event::new(format!("cumulative_trade_{i}")).add_attributes(attrs))
    }

    let trade = match &trades {
        [trade1, trade2] => match trade1.base_asset.amount.cmp(&trade2.quote_asset.amount) {
            // We received less trade1.base_asset than sold i.e. we sold trade1.base_asset
            Ordering::Less => CumulativeTrade {
                base_asset: trade1
                    .quote_asset
                    .info
                    .with_dec_balance(trade2.base_asset.amount - trade1.quote_asset.amount),
                quote_asset: trade1
                    .base_asset
                    .info
                    .with_dec_balance(trade2.quote_asset.amount - trade1.base_asset.amount),
            },
            // We received more trade1.base_asset than sold i.e. we bought trade1.quote_asset
            Ordering::Greater => CumulativeTrade {
                base_asset: trade1
                    .base_asset
                    .info
                    .with_dec_balance(trade1.base_asset.amount - trade2.quote_asset.amount),
                quote_asset: trade1
                    .quote_asset
                    .info
                    .with_dec_balance(trade1.quote_asset.amount - trade2.base_asset.amount),
            },
            Ordering::Equal => unreachable!(),
        },
        [trade] => trade.clone(),
        _ => unreachable!("Must be at least 1 and at most 2 cumulative trades"),
    };

    // Skip very small trade sizes which could significantly mess up the price due to rounding errors,
    // especially if token precisions are 18.
    if trade.base_asset.amount >= MIN_TRADE_SIZE && trade.quote_asset.amount >= MIN_TRADE_SIZE {
        let offer_ind = config
            .pair_info
            .asset_infos
            .iter()
            .position(|asset_info| asset_info == &trade.base_asset.info)
            .unwrap();
        let last_price = if offer_ind == 0 {
            trade.base_asset.amount / trade.quote_asset.amount
        } else {
            trade.quote_asset.amount / trade.base_asset.amount
        };

        let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
            .to_decimal256(LP_TOKEN_PRECISION)?;

        let ixs = [
            *balances[0],
            *balances[1] * config.pool_state.price_state.price_scale,
        ];

        config
            .pool_state
            .update_price(&config.pool_params, env, total_share, &ixs, last_price)?;
    }

    Ok(Response::default()
        .add_messages(messages)
        .add_events(events))
}

pub fn sync_pool_with_orderbook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut ob_state = OrderbookState::load(deps.storage)?;

    if let Some(executor) = &ob_state.executor {
        ensure_eq!(info.sender, executor, ContractError::Unauthorized {});
    }

    let precisions = Precisions::new(deps.storage)?;
    let mut config = CONFIG.load(deps.storage)?;
    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    if !cumulative_trades.is_empty() {
        let mut pools = liquidity.total_dec(&precisions)?;

        let xs = pools.iter().map(|a| a.amount).collect_vec();
        let old_real_price = calc_last_prices(&xs, &config, &env)?;

        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        let response = process_cumulative_trades(
            deps.as_ref(),
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;

        accumulate_prices(&env, &mut config, old_real_price);

        CONFIG.save(deps.storage, &config)?;

        let xs = pools.iter().map(|a| a.amount).collect_vec();
        let cancel_msgs = ob_state.cancel_orders(&env.contract.address);
        let order_msgs = ob_state.deploy_orders(&env, &config, &xs, &precisions)?;

        let next_contract_liquidity = xs
            .iter()
            .zip(config.pair_info.asset_infos.iter())
            .map(|(amount_dec, asset_info)| {
                let prec = precisions.get_precision(asset_info).unwrap();
                let amount = amount_dec.to_uint(prec)?;
                Ok(asset_info.with_balance(amount))
            })
            .collect::<StdResult<Vec<_>>>()?;
        let submsgs = ob_state.flatten_msgs_and_add_callback(
            &next_contract_liquidity,
            &[cancel_msgs],
            order_msgs,
        );
        ob_state.save(deps.storage)?;

        Ok(response
            .add_attribute("action", "sync_pool_with_orderbook")
            .add_submessages(submsgs))
    } else {
        Err(OrderbookError::NoNeedToSync {}.into())
    }
}
