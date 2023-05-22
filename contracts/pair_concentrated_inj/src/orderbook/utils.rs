use astroport::asset::{Asset, AssetInfo, AssetInfoExt, DecimalAsset};
use cosmwasm_std::{
    Addr, CosmosMsg, CustomMsg, CustomQuery, Decimal, Decimal256, Env, QuerierWrapper, Response,
    StdError, StdResult,
};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Debug;
use tiny_keccak::Hasher;

use crate::contract::LP_TOKEN_PRECISION;
use crate::error::ContractError;
use crate::math::calc_y;
use crate::orderbook::consts::SUBACC_NONCE;
use crate::orderbook::error::OrderbookError;
use crate::orderbook::state::OrderbookState;
use crate::state::{AmpGamma, Config, Precisions};
use astroport::cosmwasm_ext::{AbsDiff, ConvertInto, IntegerToDecimal};
use astroport::querier::{query_fee_info, query_supply};
use injective_cosmwasm::{
    checked_address_to_subaccount_id, create_batch_update_orders_msg, create_withdraw_msg,
    InjectiveMsgWrapper, InjectiveQuerier, MarketId, OrderType, SpotOrder, SubaccountId,
};

/// Calculate hash from two binary slices.
pub fn calc_hash(a1: &[u8], a2: &[u8]) -> String {
    let mut hashier = tiny_keccak::Keccak::v256();

    hashier.update(a1);
    hashier.update(a2);
    let mut output = [0u8; 32];
    hashier.finalize(&mut output);

    format!("0x{}", hex::encode(output))
}

/// Calculate available market ids for specified asset infos.
/// Currently, this pair supports only pairs thus only 2 market ids are possible.
pub fn calc_market_ids(asset_infos: &[AssetInfo]) -> StdResult<[String; 2]> {
    if asset_infos.len() != 2 {
        return Err(StdError::generic_err(
            "Orderbook integration supports only pools with 2 assets",
        ));
    }

    let assets = asset_infos
        .iter()
        .map(|asset_info| match asset_info {
            AssetInfo::Token { .. } => Err(StdError::generic_err("CW20 tokens not supported")),
            AssetInfo::NativeToken { denom } => Ok(denom.as_bytes()),
        })
        .collect::<StdResult<Vec<&[u8]>>>()?;

    Ok([
        calc_hash(assets[0], assets[1]),
        calc_hash(assets[1], assets[0]),
    ])
}

/// A thin wrapper to get subaccount we are working with.
#[inline]
pub fn get_subaccount(addr: &Addr) -> SubaccountId {
    // Starting from v1.10 injective uses default subaccount (nonce = 0) to automatically transfer
    // funds from bank module when creating an order. We need to avoid it.
    checked_address_to_subaccount_id(addr, SUBACC_NONCE)
}

/// A thin wrapper to create new batch of orders.
#[inline]
pub fn update_spot_orders(
    sender: &Addr,
    new_spot_orders: Vec<SpotOrder>,
) -> CosmosMsg<InjectiveMsgWrapper> {
    create_batch_update_orders_msg(
        sender.clone(),
        None,
        vec![],
        vec![],
        vec![],
        vec![],
        new_spot_orders,
        vec![],
    )
}

/// A thin wrapper to cancel all orders.
#[inline]
pub fn cancel_all_orders(
    sender: &Addr,
    subaccount: &SubaccountId,
    market_id: &MarketId,
) -> CosmosMsg<InjectiveMsgWrapper> {
    create_batch_update_orders_msg(
        sender.clone(),
        Some(subaccount.clone()),
        vec![market_id.clone()],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

/// Fetches subaccount balances in decimal representation.
pub(crate) fn get_subaccount_balances_dec(
    asset_infos: &[AssetInfo],
    precisions: &Precisions,
    querier: &InjectiveQuerier,
    subaccount: &SubaccountId,
) -> Result<Vec<DecimalAsset>, ContractError> {
    get_subaccount_balances(asset_infos, querier, subaccount)?
        .into_iter()
        .map(|asset| {
            let dec_asset = DecimalAsset {
                amount: asset
                    .amount
                    .to_decimal256(precisions.get_precision(&asset.info)?)?,
                info: asset.info,
            };
            Ok(dec_asset)
        })
        .collect()
}

/// Fetches subaccount balances in integer representation.
pub fn get_subaccount_balances(
    asset_infos: &[AssetInfo],
    querier: &InjectiveQuerier,
    subaccount: &SubaccountId,
) -> Result<Vec<Asset>, ContractError> {
    asset_infos
        .iter()
        .map(|asset_info| match asset_info {
            AssetInfo::NativeToken { denom } => {
                let resp = querier.query_subaccount_deposit(subaccount, denom)?;
                let dec_asset = Asset {
                    info: asset_info.clone(),
                    amount: resp.deposits.total_balance.into(),
                };
                Ok(dec_asset)
            }
            AssetInfo::Token { .. } => {
                Err(StdError::generic_err("CW20 tokens are not supported").into())
            }
        })
        .collect()
}

/// Cancels all orders and withdraws all balances from the orderbook.
pub fn leave_orderbook(
    ob_state: &OrderbookState,
    balances: Vec<Asset>,
    env: &Env,
) -> Result<Response<InjectiveMsgWrapper>, OrderbookError> {
    // Cancel all orders first
    let cancel_orders_msg = cancel_all_orders(
        &env.contract.address,
        &ob_state.subaccount,
        &ob_state.market_id,
    );

    // Withdraw all balances
    let withdraw_messages = balances
        .into_iter()
        .filter(|asset| !asset.amount.is_zero())
        .map(|asset| {
            let msg = create_withdraw_msg(
                env.contract.address.clone(),
                ob_state.subaccount.clone(),
                asset.as_coin()?,
            );
            Ok(msg)
        })
        .collect::<StdResult<Vec<_>>>()?;

    Ok(Response::new()
        .add_message(cancel_orders_msg)
        .add_messages(withdraw_messages))
}

/// Ask chain module whether contract is registered for begin blocker or not.
pub fn is_contract_active(inj_querier: &InjectiveQuerier, contract_addr: &Addr) -> StdResult<bool> {
    let reg_info = inj_querier.query_contract_registration_info(contract_addr)?;
    let active = reg_info
        .contract
        .map(|reg| reg.is_executable)
        .unwrap_or(false);

    Ok(active)
}

/// Calculate swap result using cached D.
pub fn compute_swap(
    ixs: &[Decimal256],
    offer_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    amp_gamma: AmpGamma,
    d: Decimal256,
) -> StdResult<Decimal256> {
    let offer_ind = 1 ^ ask_ind;

    let offer_amount = if offer_ind == 1 {
        offer_amount * config.pool_state.price_state.price_scale
    } else {
        offer_amount
    };

    let mut ixs = ixs.to_vec();
    ixs[offer_ind] += offer_amount;

    let new_y = calc_y(&ixs, d, &amp_gamma, ask_ind)?;
    let mut dy = ixs[ask_ind] - new_y;
    ixs[ask_ind] = new_y;

    if ask_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
    }

    let fee_rate = config.pool_params.fee(&ixs);
    let total_fee = fee_rate * dy;
    dy -= total_fee;

    Ok(dy)
}

#[derive(Debug)]
struct AstroSpotOrder {
    price: Decimal256,
    amount: Decimal256,
    is_buy: bool,
}

/// Internal structure to handle spot orders.
pub struct SpotOrdersFactory<'a> {
    market_id: &'a MarketId,
    subaccount: &'a SubaccountId,
    orders: Vec<AstroSpotOrder>,
    min_price_tick_size: Decimal256,
    precisions_ratio: Decimal256,
    base_precision: Decimal256,
}

impl<'a> SpotOrdersFactory<'a> {
    pub fn new(
        market_id: &'a MarketId,
        subaccount: &'a SubaccountId,
        min_price_tick_size: Decimal256,
        base_precision: u8,
        quote_precision: u8,
    ) -> Self {
        let quote_precision = Decimal256::from_ratio(10u64.pow(quote_precision as u32), 1u8);
        let base_precision = Decimal256::from_ratio(10u64.pow(base_precision as u32), 1u8);
        let precisions_ratio = quote_precision / base_precision;

        Self {
            market_id,
            subaccount,
            orders: vec![],
            min_price_tick_size,
            precisions_ratio,
            base_precision,
        }
    }

    /// Buy base asset with quote asset
    pub fn buy(&mut self, price: Decimal256, amount: Decimal256) {
        self.orders.push(AstroSpotOrder {
            // Adjusting price to min_price_tick_size
            price: (price / self.min_price_tick_size).floor() * self.min_price_tick_size,
            amount,
            is_buy: true,
        });
    }

    /// Sell base asset for quote asset
    pub fn sell(&mut self, price: Decimal256, amount: Decimal256) {
        self.orders.push(AstroSpotOrder {
            // Adjusting price to min_price_tick_size
            price: (price / self.min_price_tick_size).ceil() * self.min_price_tick_size,
            amount,
            is_buy: false,
        });
    }

    /// Calculate total sell/buy liquidity measured in quote asset.
    pub fn orderbook_one_side_liquidity(&self, is_buy: bool) -> Decimal256 {
        self.orders
            .iter()
            .filter(|order| order.is_buy == is_buy)
            .fold(Decimal256::zero(), |acc, order| {
                acc + order.price * order.amount
            })
    }

    /// Calculates total subaccount balance the contract will need to place all orders.
    pub(crate) fn total_deposit(
        &self,
        asset_infos: &[AssetInfo],
        precisions: &Precisions,
    ) -> Result<Vec<Asset>, ContractError> {
        let init = vec![
            // base
            asset_infos[0].with_dec_balance(Decimal256::zero()),
            // quote
            asset_infos[1].with_dec_balance(Decimal256::zero()),
        ];

        let dec_deposits = self.orders.iter().fold(init, |mut acc, order| {
            if order.is_buy {
                acc[1].amount += order.price * order.amount;
            } else {
                acc[0].amount += order.amount;
            }
            acc
        });

        dec_deposits
            .into_iter()
            .map(|dec_asset| {
                let precision = precisions.get_precision(&dec_asset.info)?;
                dec_asset.into_asset(precision).map_err(Into::into)
            })
            .collect()
    }

    /// Aggregates orders with the same price. Adjusts price to min_price_tick_size and converts
    /// orders into Injective representation.
    pub fn collect_orders(&self, fee_receiver: &Addr) -> StdResult<Vec<SpotOrder>> {
        let mut temp_orders_map = HashMap::new();

        for order in &self.orders {
            let price = if order.is_buy {
                (order.price * self.precisions_ratio / self.min_price_tick_size).floor()
                    * self.min_price_tick_size
            } else {
                (order.price * self.precisions_ratio / self.min_price_tick_size).ceil()
                    * self.min_price_tick_size
            };

            // Decimal256 doesn't implement Hash, so we use string representation of price
            let entry = temp_orders_map
                .entry((price.to_string(), order.is_buy))
                .or_insert_with(|| AstroSpotOrder {
                    amount: Decimal256::zero(),
                    price,
                    ..*order
                });
            entry.amount += order.amount;
        }

        temp_orders_map
            .values()
            .map(|order| {
                Ok(SpotOrder::new(
                    order.price.conv()?,
                    (order.amount * self.base_precision).conv()?,
                    if order.is_buy {
                        OrderType::BuyPo
                    } else {
                        OrderType::SellPo
                    },
                    self.market_id,
                    self.subaccount.clone(),
                    Some(fee_receiver.clone()),
                ))
            })
            .collect()
    }
}

/// Process filled orders as one cumulative trade. Send maker fees and run repegging algorithm.
#[allow(clippy::too_many_arguments)]
pub fn process_cumulative_trade<C, T>(
    querier: QuerierWrapper<C>,
    env: &Env,
    ob_state: &OrderbookState,
    config: &mut Config,
    pools: &mut [Decimal256],
    subacc_balances: &[Asset],
    base_precision: u8,
    quote_precision: u8,
) -> Result<Vec<CosmosMsg<T>>, OrderbookError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    let bal_diffs = ob_state
        .last_balances
        .iter()
        .zip(subacc_balances.iter())
        .map(|(a, b)| a.amount.diff(b.amount))
        .collect::<Vec<_>>();

    let mut ixs = pools.to_vec();
    // converting into internal representation
    ixs[1] *= config.pool_state.price_state.price_scale;

    let fee_info = query_fee_info(
        &querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    let mut messages = vec![];
    if let Some(fee_addr) = fee_info.fee_address {
        // This is safe conversion because fee_rate is always <= 1
        let dynamic_fee_rate: Decimal = config.pool_params.fee(&ixs).conv()?;
        let maker_fee_rate = dynamic_fee_rate * fee_info.maker_fee_rate;

        // Send maker fees
        match ob_state.last_balances[0]
            .amount
            .cmp(&subacc_balances[0].amount)
        {
            Ordering::Greater => {
                // quote -> base i.e. buy direction. Charging fees in base asset
                let maker_fee = bal_diffs[0] * maker_fee_rate;
                let maker_fee_dec = maker_fee.to_decimal256(base_precision)?;
                ixs[0] -= maker_fee_dec;
                pools[0] -= maker_fee_dec;
                messages.push(
                    config.pair_info.asset_infos[0]
                        .with_balance(maker_fee)
                        .into_msg(fee_addr)?,
                );
            }
            Ordering::Less => {
                // base -> quote i.e. sell direction. Charging fees in quote asset
                let maker_fee = bal_diffs[1] * maker_fee_rate;
                let maker_fee_dec = maker_fee.to_decimal256(quote_precision)?;
                ixs[1] -= maker_fee_dec * config.pool_state.price_state.price_scale;
                pools[1] -= maker_fee_dec;
                messages.push(
                    config.pair_info.asset_infos[1]
                        .with_balance(maker_fee)
                        .into_msg(fee_addr)?,
                );
            }
            Ordering::Equal => {
                // this should never happen as we supposed to call this function only
                // if there was at least one trade
                return Ok(messages);
            }
        }
    }

    let fba_price = bal_diffs[0].to_decimal256(base_precision)?
        / bal_diffs[1].to_decimal256(quote_precision)?;

    let total_lp = query_supply(&querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    config
        .pool_state
        .update_price(&config.pool_params, env, total_lp, &ixs, fba_price)?;

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport::asset::{native_asset_info, token_asset_info};
    use cosmwasm_std::Addr;

    #[test]
    fn test_calc_market_ids() {
        let asset_infos = vec![
            native_asset_info("uusd".to_string()),
            native_asset_info("uatom".to_string()),
        ];

        let market_ids = calc_market_ids(&asset_infos).unwrap();

        assert_eq!(
            market_ids,
            [
                "0xa5192bf894dae8417efa4c63ef7d942dce5c0ccec619ea543b8d466de7058fb2",
                "0xb166a32e3efc21c5df960ea0ce60e9232fa55a26161307879c64265dcd9aa01d"
            ]
        );
    }

    #[test]
    fn test_calc_market_ids_with_cw20() {
        let asset_infos = vec![
            native_asset_info("uusd".to_string()),
            token_asset_info(Addr::unchecked("astro".to_string())),
        ];

        let err = calc_market_ids(&asset_infos).unwrap_err();

        assert_eq!(err.to_string(), "Generic error: CW20 tokens not supported");
    }

    #[test]
    fn test_calc_market_ids_with_more_than_2_assets() {
        let asset_infos = vec![
            native_asset_info("uusd".to_string()),
            token_asset_info(Addr::unchecked("astro".to_string())),
            native_asset_info("uatom".to_string()),
        ];

        let err = calc_market_ids(&asset_infos).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Generic error: Orderbook integration supports only pools with 2 assets"
        );
    }
}
