use std::cmp::Ordering;

use cosmwasm_std::{
    ensure, ensure_eq, Addr, CosmosMsg, Decimal256, OverflowError, QuerierWrapper, StdError,
    StdResult, Uint128, Uint256,
};
use itertools::Itertools;
use neutron_std::types::neutron::dex::MsgPlaceLimitOrder;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt, Decimal256Ext, DecimalAsset};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport_pcl_common::state::Precisions;
use astroport_pcl_common::{
    calc_y,
    state::{AmpGamma, Config},
};

use crate::error::ContractError;
use crate::orderbook::execute::CumulativeTrade;
use crate::orderbook::state::OrderbookState;

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
                        self.precision[1],
                        self.precision[0],
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
    .checked_mul(Uint256::from(10u128).pow(9))?
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
        ob_state: &OrderbookState,
        force_update: bool,
    ) -> StdResult<Self> {
        Ok(Self {
            contract: config
                .pair_info
                .query_pools(&querier, &config.pair_info.contract_addr)?,
            orderbook: ob_state
                .query_ob_liquidity(querier, &config.pair_info.contract_addr, force_update)?
                .into_iter()
                .map(Asset::from)
                .collect(),
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
/// Return CumulativeTrade object which is the difference between last and current balances.
pub fn fetch_cumulative_trade(
    precisions: &Precisions,
    last_balances: &[Asset],
    new_balances: &[Asset],
) -> Result<Option<CumulativeTrade>, ContractError> {
    let mut new_balances = new_balances.to_vec();
    if !new_balances.is_empty() {
        if last_balances[0].info != new_balances[0].info {
            new_balances.swap(0, 1);
        }

        let bal_diffs = last_balances
            .iter()
            .zip(new_balances.iter())
            .map(|(a, b)| b.amount.abs_diff(a.amount))
            .collect_vec();

        let diff_to_dec_asset = |ind: usize| -> Result<_, ContractError> {
            let asset_info = &new_balances[ind].info;
            let precision = precisions.get_precision(asset_info)?;
            Ok(asset_info.with_dec_balance(bal_diffs[ind].to_decimal256(precision)?))
        };

        let maybe_trade = match last_balances[0].amount.cmp(&new_balances[0].amount) {
            // We sold asset 0 for asset 1
            Ordering::Less => {
                ensure!(
                    last_balances[1].amount > new_balances[1].amount,
                    StdError::generic_err(
                        "Invalid balance difference while calculating cumulative trade"
                    )
                );
                Some(CumulativeTrade {
                    base_asset: diff_to_dec_asset(1)?,
                    quote_asset: diff_to_dec_asset(0)?,
                })
            }
            // We bought asset 0 with asset 1
            Ordering::Greater => {
                ensure!(
                    last_balances[1].amount < new_balances[1].amount,
                    StdError::generic_err(
                        "Invalid balance difference while calculating cumulative trade"
                    )
                );
                Some(CumulativeTrade {
                    base_asset: diff_to_dec_asset(0)?,
                    quote_asset: diff_to_dec_asset(1)?,
                })
            }
            // No trade happened
            Ordering::Equal => {
                ensure_eq!(
                    last_balances[1].amount,
                    new_balances[1].amount,
                    StdError::generic_err(
                        "Invalid balance difference while calculating cumulative trade"
                    )
                );
                None
            }
        };

        Ok(maybe_trade)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod unit_tests {
    use cosmwasm_std::testing::MockStorage;

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
    fn test_cumulative_trade() {
        let mut storage = MockStorage::new();
        for (asset_info, precision) in [
            (AssetInfo::native("untrn"), 6),
            (AssetInfo::native("astro"), 8),
        ] {
            Precisions::PRECISIONS
                .save(&mut storage, asset_info.to_string(), &precision)
                .unwrap();
        }

        let precisions = Precisions::new(&storage).unwrap();
        let last_balances = vec![
            Asset::native("astro", 1000_00000000u128),
            Asset::native("untrn", 1000_000000u128),
        ];
        let new_balances = vec![
            Asset::native("untrn", 950_000000u128),
            Asset::native("astro", 1050_00000000u128),
        ];

        let trade = fetch_cumulative_trade(&precisions, &last_balances, &new_balances)
            .unwrap()
            .unwrap();
        assert_eq!(
            trade,
            CumulativeTrade {
                base_asset: AssetInfo::native("untrn").with_dec_balance(f64_to_dec(50.0)),
                quote_asset: AssetInfo::native("astro").with_dec_balance(f64_to_dec(50.0)),
            }
        );

        // Trade in opposite direction
        let new_balances = vec![
            Asset::native("untrn", 1050_000000u128),
            Asset::native("astro", 950_00000000u128),
        ];
        let trade = fetch_cumulative_trade(&precisions, &last_balances, &new_balances)
            .unwrap()
            .unwrap();
        assert_eq!(
            trade,
            CumulativeTrade {
                base_asset: AssetInfo::native("astro").with_dec_balance(f64_to_dec(50.0)),
                quote_asset: AssetInfo::native("untrn").with_dec_balance(f64_to_dec(50.0)),
            }
        );

        // No trade
        assert_eq!(
            fetch_cumulative_trade(&precisions, &last_balances, &last_balances).unwrap(),
            None
        );

        // Invalid balance for 2nd asset while 1st asset is the same
        let new_balances = vec![
            Asset::native("untrn", 1000_000000u128),
            Asset::native("astro", 950_00000000u128),
        ];
        let trade = fetch_cumulative_trade(&precisions, &last_balances, &new_balances).unwrap_err();
        assert_eq!(
            trade.to_string(),
            "Generic error: Invalid balance difference while calculating cumulative trade"
        );

        // Invalid balance for 2nd asset while 1st asset increased
        let new_balances = vec![
            Asset::native("untrn", 1050_000000u128),
            Asset::native("astro", 1000_00000000u128),
        ];
        let trade = fetch_cumulative_trade(&precisions, &last_balances, &new_balances).unwrap_err();
        assert_eq!(
            trade.to_string(),
            "Generic error: Invalid balance difference while calculating cumulative trade"
        );

        // Invalid balance for 1st asset while 2nd asset increased
        let new_balances = vec![
            Asset::native("untrn", 1001_000000u128),
            Asset::native("astro", 1050_00000000u128),
        ];
        let trade = fetch_cumulative_trade(&precisions, &last_balances, &new_balances).unwrap_err();
        assert_eq!(
            trade.to_string(),
            "Generic error: Invalid balance difference while calculating cumulative trade"
        );
    }
}
