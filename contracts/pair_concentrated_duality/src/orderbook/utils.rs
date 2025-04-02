use std::cmp::Ordering;

use cosmwasm_std::{
    Addr, Coin, CosmosMsg, Decimal256, Fraction, OverflowError, QuerierWrapper, StdError,
    StdResult, Uint128,
};
use itertools::Itertools;
use neutron_std::types::neutron::dex::MsgPlaceLimitOrder;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt, Decimal256Ext, DecimalAsset};
use astroport_pcl_common::state::Precisions;
use astroport_pcl_common::{
    calc_d, calc_y,
    state::{AmpGamma, Config},
};

use crate::error::ContractError;
use crate::orderbook::consts::DUALITY_PRICE_ADJUSTMENT;
use crate::orderbook::state::{OrderState, OrderbookState};

pub fn compute_offer_amount(
    ixs: &[Decimal256],
    ask_amount: Decimal256,
    offer_ind: usize,
    config: &Config,
    amp_gamma: AmpGamma,
    d: Decimal256,
) -> StdResult<Decimal256> {
    let ask_ind = 1 ^ offer_ind;

    let ask_amount = if ask_ind == 1 {
        ask_amount * config.pool_state.price_state.price_scale
    } else {
        ask_amount
    };

    let mut ixs = ixs.to_vec();

    // It's hard to predict fee rate thus we use maximum possible fee rate
    let before_fee = ask_amount
        * (Decimal256::one() - Decimal256::from(config.pool_params.out_fee))
            .inv()
            .unwrap();

    ixs[ask_ind] -= before_fee;

    let new_y = calc_y(&ixs, d, &amp_gamma, offer_ind)?;
    let mut dx = new_y - ixs[offer_ind];

    if offer_ind == 1 {
        dx /= config.pool_state.price_state.price_scale;
    }

    Ok(dx)
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
    avg_price_adjustment: Decimal256,
}

impl SpotOrdersFactory {
    pub fn new(
        asset_infos: &[AssetInfo],
        asset_0_precision: u8,
        asset_1_precision: u8,
        avg_price_adjustment: Decimal256,
    ) -> Self {
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
            avg_price_adjustment,
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

    /// Create internal orders using equal heights algorithm.
    /// Returns boolean indicating whether the orders were successfully created.
    pub fn construct_orders(
        &mut self,
        pair_config: &Config,
        amp_gamma: AmpGamma,
        ixs: &[Decimal256],
        asset_0_trade_size: Decimal256,
        asset_1_trade_size: Decimal256,
        orders_number: u8,
    ) -> Result<bool, ContractError> {
        let d = calc_d(ixs, &amp_gamma)?;

        for i in 1..=orders_number {
            let i_dec = Decimal256::from_integer(i);

            let asset_0_sell_amount = asset_0_trade_size * i_dec;
            let asset_1_sell_amount =
                compute_offer_amount(ixs, asset_0_sell_amount, 1, pair_config, amp_gamma, d)?;

            let sell_price = if i > 1 {
                asset_1_sell_amount.checked_sub(self.orderbook_one_side_liquidity(false))?
                    / asset_0_trade_size
            } else {
                asset_1_sell_amount / asset_0_sell_amount
            };

            let asset_1_buy_amount = asset_1_trade_size * i_dec;
            let asset_0_buy_amount =
                compute_offer_amount(ixs, asset_1_buy_amount, 0, pair_config, amp_gamma, d)?;

            let buy_price = if i > 1 {
                asset_0_buy_amount.checked_sub(self.orderbook_one_side_liquidity(true))?
                    / asset_1_trade_size
            } else {
                asset_0_buy_amount / asset_1_buy_amount
            };

            // If at some point the price becomes zero, we don't post new orders
            if sell_price.is_zero() || buy_price.is_zero() {
                return Ok(false);
            }

            self.sell(sell_price, asset_0_trade_size);
            self.buy(buy_price, asset_1_trade_size);
        }

        Ok(true)
    }

    pub fn total_exported_liquidity(&self) -> StdResult<Vec<Asset>> {
        let mut liquidity = self
            .orders
            .iter()
            .map(|order| {
                let asset = if order.is_buy {
                    Asset::native(
                        &self.denoms[1],
                        Uint128::try_from((order.amount * self.multiplier[1]).to_uint_floor())?,
                    )
                } else {
                    Asset::native(
                        &self.denoms[0],
                        Uint128::try_from((order.amount * self.multiplier[0]).to_uint_floor())?,
                    )
                };

                Ok(asset)
            })
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .into_group_map_by(|asset| asset.info.clone())
            .into_iter()
            .map(|(info, assets)| {
                let sum = assets.iter().fold(Uint128::zero(), |acc, a| acc + a.amount);
                info.with_balance(sum)
            })
            .collect_vec();

        if liquidity[0].info.to_string() != self.denoms[0] {
            liquidity.swap(0, 1);
        }

        Ok(liquidity)
    }

    pub fn collect_spot_orders(self, sender: &Addr) -> Vec<CosmosMsg> {
        self.orders
            .into_iter()
            .map(|order| {
                // Worsen the price to make sure rounding errors are covered in favor of our pool
                let limit_price = order.price + self.avg_price_adjustment * order.price;

                if order.is_buy {
                    let limit_sell_price = price_to_duality_notation(
                        limit_price,
                        self.precision[1],
                        self.precision[0],
                    )
                    .unwrap();
                    let min_average_sell_price = price_to_duality_notation(
                        order.price,
                        self.precision[1],
                        self.precision[0],
                    )
                    .unwrap();

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
                        limit_sell_price: Some(limit_sell_price.clone()),
                        tick_index_in_to_out: 0,
                        min_average_sell_price: Some(min_average_sell_price),
                    }
                    .into()
                } else {
                    let limit_sell_price = price_to_duality_notation(
                        limit_price,
                        self.precision[0],
                        self.precision[1],
                    )
                    .unwrap();
                    let min_average_sell_price = price_to_duality_notation(
                        order.price,
                        self.precision[0],
                        self.precision[1],
                    )
                    .unwrap();

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
                        limit_sell_price: Some(limit_sell_price.clone()),
                        tick_index_in_to_out: 0,
                        min_average_sell_price: Some(min_average_sell_price),
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
        Ordering::Less => {
            price / Decimal256::from_integer(10u128.pow(prec_diff.unsigned_abs() as u32))
        }
        Ordering::Equal => price,
        Ordering::Greater => price * Decimal256::from_integer(10u128.pow(prec_diff as u32)),
    }
    .atomics()
    .checked_mul(DUALITY_PRICE_ADJUSTMENT)?
    .to_string();

    Ok(price)
}

#[derive(Debug)]
pub struct Liquidity {
    pub contract: Vec<Asset>,
    pub orderbook: Vec<Asset>,
}

impl Liquidity {
    pub fn new(
        querier: QuerierWrapper,
        config: &Config,
        ob_state: &mut OrderbookState,
        force_update: bool,
    ) -> StdResult<Self> {
        Ok(Self {
            contract: config
                .pair_info
                .query_pools(&querier, &config.pair_info.contract_addr)?,
            orderbook: ob_state.query_ob_liquidity(
                querier,
                &config.pair_info.contract_addr,
                force_update,
            )?,
        })
    }

    pub fn total(&self) -> Vec<Asset> {
        let mut balances = self
            .contract
            .iter()
            .chain(self.orderbook.iter())
            .into_group_map_by(|asset| asset.info.clone())
            .into_iter()
            .map(|(info, assets)| {
                let sum = assets.iter().fold(Uint128::zero(), |acc, a| acc + a.amount);
                info.with_balance(sum)
            })
            .collect_vec();

        if balances[0].info != self.contract[0].info {
            balances.swap(0, 1);
        }

        balances
    }

    pub fn total_dec(&self, precisions: &Precisions) -> Result<Vec<DecimalAsset>, ContractError> {
        self.total()
            .into_iter()
            .map(|asset| {
                asset
                    .to_decimal_asset(precisions.get_precision(&asset.info)?)
                    .map_err(Into::into)
            })
            .collect()
    }
}

/// Checking whether there is a difference between the last and current balances.
/// Return OrderState object which is the difference between last and current balances.
pub fn fetch_autoexecuted_trade(
    last_balances: &[Asset],
    new_balances: &[Asset],
) -> StdResult<OrderState> {
    let mut new_balances = new_balances.to_vec();
    if last_balances[0].info != new_balances[0].info {
        new_balances.swap(0, 1);
    }

    let trade = if last_balances[0].amount < new_balances[0].amount {
        // We sold asset 1 for asset 0
        Ok(OrderState {
            taker_coin_out: Coin {
                denom: last_balances[0].info.to_string(),
                amount: new_balances[0].amount - last_balances[0].amount,
            },
            maker_coin_out: Coin {
                denom: last_balances[1].info.to_string(),
                // auto-executed order guarantees that maker side is fully consumed
                amount: Uint128::zero(),
            },
        })
    } else if last_balances[1].amount < new_balances[1].amount {
        // We sold asset 0 for asset 1
        Ok(OrderState {
            taker_coin_out: Coin {
                denom: last_balances[1].info.to_string(),
                amount: new_balances[1].amount - last_balances[1].amount,
            },
            maker_coin_out: Coin {
                denom: last_balances[0].info.to_string(),
                // auto-executed order guarantees that maker side is fully consumed
                amount: Uint128::zero(),
            },
        })
    } else {
        // No trade happened
        Err(StdError::generic_err(
            "PCL pool lost its liquidity in orderbook",
        ))
    }?;

    Ok(trade)
}

#[cfg(test)]
mod unit_tests {
    use cosmwasm_std::coin;

    use astroport::asset::PairInfo;
    use astroport::factory::PairType;
    use astroport_pcl_common::calc_d;
    use astroport_pcl_common::state::{PoolParams, PoolState, PriceState};
    use astroport_test::convert::f64_to_dec;

    use super::*;

    #[test]
    fn test_sci_notation_conversion() {
        let price = Decimal256::from_ratio(1u8, 3000u64);
        let base_precision = 6;
        let quote_precision = 18;
        assert_eq!(
            price_to_duality_notation(price, base_precision, quote_precision).unwrap(),
            "333333333333333000000000000000000000"
        );

        let price = Decimal256::from_ratio(3000u64, 1u8);
        let base_precision = 18;
        let quote_precision = 6;
        assert_eq!(
            price_to_duality_notation(price, base_precision, quote_precision).unwrap(),
            "3000000000000000000"
        );

        let price = Decimal256::from_ratio(1u8, 2u8);
        let base_precision = 6;
        let quote_precision = 6;
        assert_eq!(
            price_to_duality_notation(price, base_precision, quote_precision).unwrap(),
            "500000000000000000000000000"
        );
    }

    #[test]
    fn test_autoexecuted_trade() {
        let last_balances = vec![
            Asset::native("astro", 1000_000000u128),
            Asset::native("untrn", 1000_000000u128),
        ];
        let new_balances = vec![
            Asset::native("untrn", 1000_000000u128),
            Asset::native("astro", 1050_000000u128),
        ];

        let trade = fetch_autoexecuted_trade(&last_balances, &new_balances).unwrap();
        assert_eq!(
            trade,
            OrderState {
                maker_coin_out: coin(0, "untrn"),
                taker_coin_out: coin(50_000000, "astro"),
            }
        );

        // Trade in opposite direction
        let new_balances = vec![
            Asset::native("untrn", 1050_000000u128),
            Asset::native("astro", 1000_000000u128),
        ];
        let trade = fetch_autoexecuted_trade(&last_balances, &new_balances).unwrap();
        assert_eq!(
            trade,
            OrderState {
                maker_coin_out: coin(0, "astro"),
                taker_coin_out: coin(50_000000, "untrn"),
            }
        );

        let err = fetch_autoexecuted_trade(&last_balances, &last_balances).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("PCL pool lost its liquidity in orderbook")
        );
    }

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

    #[test]
    fn check_equal_heights_algo() {
        let pair_config = Config {
            pair_info: PairInfo {
                asset_infos: vec![AssetInfo::native("foo"), AssetInfo::native("bar")],
                contract_addr: Addr::unchecked(""),
                liquidity_token: "".to_string(),
                pair_type: PairType::Custom("".to_string()),
            },
            factory_addr: Addr::unchecked(""),
            block_time_last: 0,
            cumulative_prices: vec![],
            pool_params: PoolParams {
                mid_fee: f64_to_dec(0.0026),
                out_fee: f64_to_dec(0.0045),
                fee_gamma: f64_to_dec(0.00023),
                ..Default::default()
            },
            pool_state: PoolState {
                initial: Default::default(),
                future: Default::default(),
                future_time: 0,
                initial_time: 0,
                price_state: PriceState {
                    price_scale: Decimal256::from_ratio(2u8, 1u8),
                    ..Default::default()
                },
            },
            owner: None,
            track_asset_balances: false,
            fee_share: None,
            tracker_addr: None,
        };
        let orders_number = 5;
        let asset_0_trade_size = f64_to_dec(400.0);
        let asset_1_trade_size = f64_to_dec(200.0);
        let amp_gamma = AmpGamma {
            amp: f64_to_dec(10f64),
            gamma: f64_to_dec(0.000145),
        };

        let mut orders_factory =
            SpotOrdersFactory::new(&pair_config.pair_info.asset_infos, 6, 6, Decimal256::raw(1));

        let ixs = [f64_to_dec(20_000.0), f64_to_dec(20_000.0)];
        let d = calc_d(&ixs, &amp_gamma).unwrap();

        orders_factory
            .construct_orders(
                &pair_config,
                amp_gamma,
                &ixs,
                asset_0_trade_size,
                asset_1_trade_size,
                orders_number,
            )
            .unwrap();

        // How much one needs to pay to buy all asset_0 from orderbook?
        let orderbook_result = orders_factory.orderbook_one_side_liquidity(false);
        // How much would they receive from PCL?
        let pcl_result =
            compute_swap(&ixs, orderbook_result, 0, &pair_config, amp_gamma, d).unwrap();

        assert!(
            pcl_result >= asset_0_trade_size * Decimal256::from_integer(5u8),
            "Orderbook trades at discount from PCL: {pcl_result} <= {orderbook_result}",
        );

        // How much one needs to pay to buy all asset_1?
        let orderbook_result = orders_factory.orderbook_one_side_liquidity(true);
        // How much would they receive from PCL?
        let pcl_result =
            compute_swap(&ixs, orderbook_result, 1, &pair_config, amp_gamma, d).unwrap();

        assert!(
            pcl_result >= asset_1_trade_size * Decimal256::from_integer(5u8),
            "Orderbook trades at discount from PCL: {pcl_result} <= {orderbook_result}",
        );
    }
}
