use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure_eq, to_json_string, Decimal256, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
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
use crate::orderbook::utils::{fetch_cumulative_trade, Liquidity};
use crate::state::CONFIG;

use super::error::OrderbookError;
use super::state::OrderbookState;

/// CumulativeTrade represents all trades that happened on orderbook as one trade.
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

pub fn process_cumulative_trade(
    deps: Deps,
    env: &Env,
    trade: &CumulativeTrade,
    config: &mut Config,
    balances: &mut [&mut Decimal256],
    precisions: &Precisions,
    fee_info: Option<&FeeInfo>,
) -> Result<Response, OrderbookError> {
    let offer_ind = config
        .pair_info
        .asset_infos
        .iter()
        .position(|asset_info| asset_info == &trade.base_asset.info)
        .unwrap();
    let ask_ind = 1 ^ offer_ind;

    let ixs = [
        *balances[0],
        *balances[1] * config.pool_state.price_state.price_scale,
    ];
    let fee_rate = config.pool_params.fee(&ixs);
    let total_fee = fee_rate * trade.quote_asset.amount;

    let ask_asset_prec = precisions.get_precision(&config.pair_info.asset_infos[ask_ind])?;
    let mut messages = vec![];
    let mut attrs = vec![(
        "cumulative_trade",
        to_json_string(&trade.try_into_uint(precisions)?)?,
    )];

    let mut share_amount = Decimal256::zero();
    // Send the shared fee
    if let Some(fee_share) = &config.fee_share {
        share_amount = total_fee * Decimal256::from_ratio(fee_share.bps, 10000u16);
        *balances[ask_ind] -= share_amount;

        let fee_share_amount = share_amount.to_uint(ask_asset_prec)?;
        if !fee_share_amount.is_zero() {
            let fee = config.pair_info.asset_infos[ask_ind].with_balance(fee_share_amount);
            attrs.push(("fee_share_amount", fee.to_string()));
            messages.push(fee.into_msg(&fee_share.recipient)?);
        }
    }

    let fee_info = if let Some(fee_info) = fee_info.cloned() {
        fee_info
    } else {
        query_fee_info(
            &deps.querier,
            &config.factory_addr,
            config.pair_info.pair_type.clone(),
        )?
    };
    // Send the maker fee
    if let Some(fee_address) = &fee_info.fee_address {
        let maker_share = (total_fee - share_amount) * Decimal256::from(fee_info.maker_fee_rate);
        *balances[ask_ind] -= maker_share;

        let maker_fee = maker_share.to_uint(ask_asset_prec)?;
        if !maker_fee.is_zero() {
            let fee = config.pair_info.asset_infos[ask_ind].with_balance(maker_fee);
            attrs.push(("maker_fee_amount", fee.to_string()));
            messages.push(fee.into_msg(fee_address)?);
        }
    }

    // Skip very small trade sizes which could significantly mess up the price due to rounding errors,
    // especially if token precisions are 18.
    if trade.base_asset.amount >= MIN_TRADE_SIZE && trade.quote_asset.amount >= MIN_TRADE_SIZE {
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
        .add_attributes(attrs))
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
    let liquidity = Liquidity::new(deps.querier, &config, &ob_state, false)?;

    if let Some(cumulative_trade) =
        fetch_cumulative_trade(&precisions, &ob_state.last_balances, &liquidity.orderbook)?
    {
        let mut pools = liquidity.total_dec(&precisions)?;

        let xs = pools.iter().map(|a| a.amount).collect_vec();
        let old_real_price = calc_last_prices(&xs, &config, &env)?;

        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        let response = process_cumulative_trade(
            deps.as_ref(),
            &env,
            &cumulative_trade,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;

        accumulate_prices(&env, &mut config, old_real_price);

        CONFIG.save(deps.storage, &config)?;

        let cancel_msgs = ob_state.cancel_orders(&env.contract.address);

        let balances = pools.iter().map(|asset| asset.amount).collect_vec();
        let order_msgs = ob_state.deploy_orders(&env, &config, &balances, &precisions)?;

        let pools_u128 = pools
            .iter()
            .map(|asset| {
                let prec = precisions.get_precision(&asset.info).unwrap();
                let amount = asset.amount.to_uint(prec)?;
                Ok(asset.info.with_balance(amount))
            })
            .collect::<StdResult<Vec<_>>>()?;
        let submsgs =
            ob_state.flatten_msgs_and_add_callback(&pools_u128, &[cancel_msgs], order_msgs);

        Ok(response
            .add_attribute("action", "sync_pool_with_orderbook")
            .add_submessages(submsgs))
    } else {
        Err(OrderbookError::NoNeedToSync {}.into())
    }
}
