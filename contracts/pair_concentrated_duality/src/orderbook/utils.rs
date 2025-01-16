use std::cmp::Ordering;

use cosmwasm_std::{Addr, Api, CosmosMsg, Decimal256, OverflowError, StdResult, Uint256};
use neutron_std::types::neutron::dex::MsgPlaceLimitOrder;

use astroport::asset::{AssetInfo, Decimal256Ext};
use astroport_pcl_common::{
    calc_y,
    state::{AmpGamma, Config},
};

/// Calculate the swap result using cached D.
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

/// Internal structure to handle spot orders
pub struct SpotOrdersFactory {
    orders: Vec<AstroSpotOrder>,
    multiplier: [Decimal256; 2],
    precision: [u8; 2],
    denoms: Vec<String>,
}

impl SpotOrdersFactory {
    pub fn new(asset_infos: &[AssetInfo], asset_0_precision: u8, asset_1_precision: u8) -> Self {
        let denoms = asset_infos
            .iter()
            .map(|info| match &info {
                AssetInfo::Token { .. } => unreachable!("cw20 token is not supported"),
                AssetInfo::NativeToken { denom } => denom.clone(),
            })
            .collect();

        Self {
            orders: vec![],
            multiplier: [
                Decimal256::from_integer(10u64.pow(asset_0_precision as u32)),
                Decimal256::from_integer(10u64.pow(asset_1_precision as u32)),
            ],
            precision: [asset_0_precision, asset_1_precision],
            denoms,
        }
    }

    /// Buy asset_0 with asset_1
    /// (Sell asset_1 for asset_0)
    pub fn buy(&mut self, price: Decimal256, amount: Decimal256) {
        self.orders.push(AstroSpotOrder {
            price,
            amount,
            is_buy: true,
        });
    }

    /// Sell asset_0 for asset_1
    /// (Buy asset_1 with asset_0)
    pub fn sell(&mut self, price: Decimal256, amount: Decimal256) {
        self.orders.push(AstroSpotOrder {
            price,
            amount,
            is_buy: false,
        });
    }

    /// Calculate total sell/buy liquidity in one side
    pub fn orderbook_one_side_liquidity(&self, is_buy: bool) -> Decimal256 {
        self.orders
            .iter()
            .filter(|order| order.is_buy == is_buy)
            .fold(Decimal256::zero(), |acc, order| {
                acc + order.price * order.amount
            })
    }

    pub fn collect_spot_orders(self, sender: &Addr, api: &dyn Api) -> Vec<CosmosMsg> {
        self.orders
            .into_iter()
            .map(|order| {
                if order.is_buy {
                    let limit_sell_price = price_to_duality_notation(
                        order.price,
                        self.precision[1],
                        self.precision[0],
                    )
                    .unwrap();

                    api.debug(&format!(
                        "buy: limit_sell_price: {limit_sell_price}, amount_in: {}",
                        (order.amount * self.multiplier[1])
                            .to_uint_floor()
                            .to_string()
                    ));

                    #[allow(deprecated)]
                    MsgPlaceLimitOrder {
                        creator: sender.to_string(),
                        amount_in: (order.amount * self.multiplier[1])
                            .to_uint_floor()
                            .to_string(),
                        // order_type: LimitOrderType::GoodTilCancelled,
                        order_type: 0, // https://github.com/neutron-org/neutron/blob/main/proto/neutron/dex/tx.proto#L126
                        max_amount_out: None,
                        expiration_time: None,
                        receiver: sender.to_string(),
                        token_in: self.denoms[1].clone(),
                        token_out: self.denoms[0].clone(),
                        limit_sell_price: Some(limit_sell_price),
                        tick_index_in_to_out: 0,
                        min_average_sell_price: None,
                    }
                    .into()
                } else {
                    let limit_sell_price = price_to_duality_notation(
                        order.price,
                        self.precision[0],
                        self.precision[1],
                    )
                    .unwrap();

                    api.debug(&format!(
                        "sell: limit_sell_price: {limit_sell_price}, amount_in: {}",
                        (order.amount * self.multiplier[0])
                            .to_uint_floor()
                            .to_string()
                    ));

                    #[allow(deprecated)]
                    MsgPlaceLimitOrder {
                        creator: sender.to_string(),
                        amount_in: (order.amount * self.multiplier[0])
                            .to_uint_floor()
                            .to_string(),
                        // order_type: LimitOrderType::GoodTilCancelled,
                        order_type: 0, // https://github.com/neutron-org/neutron/blob/main/proto/neutron/dex/tx.proto#L126
                        max_amount_out: None,
                        expiration_time: None,
                        receiver: sender.to_string(),
                        token_in: self.denoms[0].clone(),
                        token_out: self.denoms[1].clone(),
                        limit_sell_price: Some(limit_sell_price),
                        tick_index_in_to_out: 0,
                        min_average_sell_price: None,
                    }
                    .into()
                }
            })
            .collect()
    }
}

/// Converting [`Decimal256`] price to duality price notation which is
/// float multiplied by 10^27.
fn price_to_duality_notation(
    price: Decimal256,
    base_precision: u8,
    quote_precision: u8,
) -> Result<String, OverflowError> {
    let prec_diff = quote_precision as i8 - base_precision as i8;
    let price = match prec_diff.cmp(&0) {
        Ordering::Less => price / Decimal256::from_integer(10u128.pow(prec_diff as u32)),
        Ordering::Equal => price,
        Ordering::Greater => price * Decimal256::from_integer(10u128.pow(prec_diff as u32)),
    }
    .atomics()
    .checked_mul(Uint256::from(10u128).pow(9))?
    .to_string();

    Ok(price)
}

// TODO: fix tests
// #[cfg(test)]
// mod unit_tests {
//     use super::*;
//
//     #[test]
//     fn test_sci_notation_conversion() {
//         let price = Decimal256::from_ratio(1u8, 3000u64);
//         let base_precision = 6;
//         let quote_precision = 18;
//         assert_eq!(
//             price_to_sci_notation(price, base_precision, quote_precision),
//             "333333333.333333"
//         );
//
//         let price = Decimal256::from_ratio(3000u64, 1u8);
//         let base_precision = 18;
//         let quote_precision = 6;
//         assert_eq!(
//             price_to_sci_notation(price, base_precision, quote_precision),
//             "3000E-12"
//         );
//
//         let price = Decimal256::from_ratio(1u8, 2u8);
//         let base_precision = 6;
//         let quote_precision = 6;
//         assert_eq!(
//             price_to_sci_notation(price, base_precision, quote_precision),
//             "0.5"
//         );
//     }
// }
