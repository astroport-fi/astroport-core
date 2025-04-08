#![cfg(feature = "test-tube")]

use std::str::FromStr;

use cosmwasm_std::{coin, Decimal, Fraction, Uint128};
use neutron_test_tube::{Account, NeutronTestApp};
use proptest::prelude::*;

use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::pair_concentrated_duality::OrderbookConfig;
use astroport_test::coins::TestCoin;
use astroport_test::convert::dec_to_f64;
use Event::*;

use crate::common::astroport_wrapper::AstroportHelper;
use crate::common::helper::common_pcl_params;
use crate::common::neutron_wrapper::TestAppWrapper;

mod common;

const MAX_AMOUNT: u128 = 10_000_000_000000;
const MAX_EVENTS: usize = 100;

#[derive(Debug)]
enum Event {
    Provide {
        coin_0_amount: u128,
        coin_1_price_scale_divergence: u16,
    },
    Withdraw {
        percent_bps: u16,
    },
    AstroSwap {
        coin_in_index: u8,
        percent_bps: u16, // percentage of pool liquidity
    },
    DexSwap {
        coin_in_index: u8,
        percent_bps: u16,          // percentage of pool liquidity
        price_divergence_bps: i16, // divergence from last price in PCL
        sync_pool_after: bool,
    },
}

fn simulate_case(events: Vec<Event>, neutron: &TestAppWrapper) {
    // Using randomly generated address as a source of token denoms uniqueness
    let address = neutron.app.init_account(&[]).unwrap().address();

    let test_coins = vec![
        TestCoin::native(&format!("a{}", address[address.len() - 6..].to_string())),
        TestCoin::native(&format!("b{}", address[address.len() - 6..].to_string())),
    ];

    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        neutron,
        test_coins.clone(),
        ConcentratedPoolParams {
            price_scale: Decimal::from_ratio(2u8, 1u8),
            ..common_pcl_params()
        },
        OrderbookConfig {
            executor: Some(owner),
            liquidity_percent: Decimal::percent(20),
            orders_number: 3,
            min_asset_0_order_size: Uint128::from(1_000u128),
            min_asset_1_order_size: Uint128::from(1_000u128),
            avg_price_adjustment: Decimal::from_str("0.0001").unwrap(),
        },
    )
    .unwrap();

    astroport.enable_orderbook(&astroport.owner, true).unwrap();

    let user = astroport
        .helper
        .app
        .init_account(&[
            coin(u128::MAX / 2, "untrn"),
            coin(u128::MAX / 2, test_coins[0].denom().unwrap()),
            coin(u128::MAX / 2, test_coins[1].denom().unwrap()),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[0]].with_balance(1_000_000_000000u128),
        astroport.assets[&test_coins[1]].with_balance(500_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    for (i, event) in events.into_iter().enumerate() {
        println!("Event {i}: {event:?}");

        match event {
            Provide {
                coin_0_amount,
                coin_1_price_scale_divergence,
            } => {
                let price_scale: Decimal = astroport
                    .query_config()
                    .unwrap()
                    .pool_state
                    .price_state
                    .price_scale
                    .try_into()
                    .unwrap();
                let price_scale_reduce =
                    Decimal::one() - Decimal::from_ratio(coin_1_price_scale_divergence, 10_000u16);

                astroport
                    .provide_liquidity(
                        &user,
                        &[
                            astroport.assets[&test_coins[0]].with_balance(coin_0_amount),
                            astroport.assets[&test_coins[1]].with_balance(
                                price_scale_reduce * price_scale * Uint128::from(coin_0_amount),
                            ),
                        ],
                    )
                    .unwrap();
            }
            Withdraw { percent_bps } => {
                let mut lp_coin = astroport
                    .helper
                    .query_balance(&user.address(), &astroport.lp_token)
                    .unwrap();

                if !lp_coin.amount.is_zero() {
                    let share = Decimal::from_ratio(percent_bps, 10_000u16);
                    lp_coin.amount = lp_coin.amount * share;

                    astroport.withdraw_liquidity(&user, lp_coin).unwrap();
                }
            }
            AstroSwap {
                coin_in_index,
                percent_bps,
            } => {
                let coin_in = &test_coins[coin_in_index as usize];

                let coin_in_balance = astroport
                    .helper
                    .query_balance(
                        astroport.pair_addr.as_str(),
                        &astroport.assets[coin_in].to_string(),
                    )
                    .unwrap()
                    .amount;

                let amount = coin_in_balance * Decimal::from_ratio(percent_bps, 10_000u16);

                // println!(
                //     "AMM swap {}",
                //     astroport.assets[coin_in].with_balance(amount)
                // );

                if let Err(err) = astroport
                    .swap_max_spread(&user, &astroport.assets[coin_in].with_balance(amount))
                {
                    if err
                        .root_cause()
                        .to_string()
                        .contains("Operation exceeds max spread limit")
                    {
                        println!("exceeds spread limit");
                    } else {
                        panic!("{err}");
                    }
                }
            }
            DexSwap {
                coin_in_index,
                percent_bps,
                price_divergence_bps,
                sync_pool_after,
            } => {
                let coin_in = &test_coins[coin_in_index as usize];
                let coin_out = &test_coins[1 ^ coin_in_index as usize];

                let coin_in_balance = astroport
                    .helper
                    .query_balance(
                        astroport.pair_addr.as_str(),
                        &astroport.assets[coin_in].to_string(),
                    )
                    .unwrap()
                    .amount;

                let amount = coin_in_balance * Decimal::from_ratio(percent_bps, 10_000u16);

                let swap_asset = astroport.assets[coin_in].with_balance(amount);

                // println!(
                //     "Swap simulation {}",
                //     astroport
                //         .simulate_swap(swap_asset.clone())
                //         .unwrap()
                //         .return_amount
                // );

                let config = astroport.query_config().unwrap();

                let last_price: Decimal =
                    if astroport.assets[coin_in] == config.pair_info.asset_infos[0] {
                        config
                            .pool_state
                            .price_state
                            .last_price
                            .inv()
                            .unwrap()
                            .try_into()
                            .unwrap()
                    } else {
                        config.pool_state.price_state.last_price.try_into().unwrap()
                    };

                let price_divergence =
                    Decimal::from_ratio(price_divergence_bps.abs() as u16, 10_000u16);
                let price = if price_divergence_bps > 0 {
                    (Decimal::one() + price_divergence) * last_price
                } else {
                    (Decimal::one() - price_divergence) * last_price
                };

                // println!("DEX swap {} with price: {price}", &swap_asset);
                //
                // println!(
                //     "OB liquidity before DEX swap: {:?}",
                //     astroport
                //         .helper
                //         .query_total_ob_liquidity(astroport.pair_addr.as_str())
                //         .unwrap()
                // );

                if let Err(err) = astroport.helper.limit_order(
                    &user,
                    swap_asset.as_coin().unwrap(),
                    &coin_out.denom().unwrap(),
                    dec_to_f64(price),
                ) {
                    if !err
                        .to_string()
                        .contains("Trade cannot be filled at the specified LimitPrice")
                    {
                        panic!("{err}");
                    }
                }

                // println!(
                //     "OB liquidity after DEX swap: {:?}",
                //     astroport
                //         .helper
                //         .query_total_ob_liquidity(astroport.pair_addr.as_str())
                //         .unwrap()
                // );

                if sync_pool_after {
                    if let Err(err) = astroport.sync_orders(&neutron.signer) {
                        if !err.to_string().contains("Orderbook is already synced") {
                            panic!("{err}");
                        }
                    }
                }
            }
        }
    }
}

fn generate_cases() -> impl Strategy<Value = Vec<Event>> {
    let amount_strategy = 1_000..MAX_AMOUNT;
    let swap_percent_bps_strategy = 1..=3_000u16;
    let withdraw_percent_bps_strategy = 1..=5_000u16;
    let percent_bps_dex_size_strategy = 1..=10_000u16;
    let price_scale_divergence_strategy = 0..=5_000u16;
    let coin_index_strategy = 0..=1u8;
    let dex_price_divergence_strategy = -9_999i16..=30_000i16;

    let events_strategy = prop_oneof![
        (amount_strategy, price_scale_divergence_strategy).prop_map(
            |(coin_0_amount, coin_1_price_scale_divergence)| {
                Provide {
                    coin_0_amount,
                    coin_1_price_scale_divergence,
                }
            }
        ),
        withdraw_percent_bps_strategy
            .clone()
            .prop_map(|percent_bps| Withdraw { percent_bps }),
        (coin_index_strategy.clone(), swap_percent_bps_strategy).prop_map(
            |(coin_in_index, percent_bps)| AstroSwap {
                coin_in_index,
                percent_bps,
            }
        ),
        (
            coin_index_strategy,
            percent_bps_dex_size_strategy,
            dex_price_divergence_strategy,
            prop::bool::ANY
        )
            .prop_map(
                |(coin_in_index, percent_bps, price_divergence_bps, sync_pool_after)| DexSwap {
                    coin_in_index,
                    percent_bps,
                    price_divergence_bps,
                    sync_pool_after,
                }
            ),
    ];

    prop::collection::vec(events_strategy, 1..=MAX_EVENTS)
}

#[test]
fn simulate() {
    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();

    proptest!(|(events in generate_cases())| {
        simulate_case(events, &neutron);
    });
}

#[test]
fn single_test() {
    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();

    simulate_case(include!("test_case"), &neutron)
}
