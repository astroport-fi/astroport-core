#![cfg(not(tarpaulin_include))]
#![cfg(feature = "test-tube")]
#![allow(dead_code)]

use std::str::FromStr;

use cosmwasm_schema::schemars::_serde_json::from_str;
use cosmwasm_std::{coin, Decimal, Decimal256, Uint128};
use itertools::Itertools;
use neutron_test_tube::{Account, NeutronTestApp};

use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::pair_concentrated_duality::OrderbookConfig;
use astroport_pair_concentrated_duality::orderbook::execute::CumulativeTradeUint;
use astroport_test::coins::TestCoin;
use common::{
    astroport_wrapper::AstroportHelper, helper::common_pcl_params, neutron_wrapper::TestAppWrapper,
};

mod common;

#[test]
fn test_basic_ops() {
    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("astro")];
    let orders_number = 1;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
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
            liquidity_percent: Decimal::percent(5),
            orders_number,
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
            coin(2_000_000_000000u128, "untrn"),
            coin(505_000_000000u128, "astro"),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[1]].with_balance(500_000_000000u128),
        astroport.assets[&test_coins[0]].with_balance(1_000_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    assert_eq!(astroport.pool_balances().unwrap().assets, initial_balances);

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap()
        .limit_orders
        .into_iter()
        .map(|order| order.tranche_key)
        .collect_vec();
    assert_eq!(orders.len(), (orders_number * 2) as usize);
    let ob_liquidity = astroport
        .helper
        .query_total_ob_liquidity(astroport.pair_addr.as_str())
        .unwrap()
        .into_iter()
        .sorted_by(|a, b| a.denom.cmp(&b.denom))
        .collect_vec();
    assert_eq!(
        ob_liquidity,
        [coin(12500_000000, "astro"), coin(25000_000000, "untrn")]
    );

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(1_000_000000u128);
    astroport.swap(&user, &swap_asset, None).unwrap();

    assert_eq!(
        astroport.pool_balances().unwrap().assets,
        [
            astroport.assets[&test_coins[1]].with_balance(501_000_000000u128),
            astroport.assets[&test_coins[0]].with_balance(998_002_684482u128),
        ]
    );

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
        ])
        .unwrap();

    // DEX swap NTRN -> ASTRO
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(1_000_000000u128, "untrn"), "astro", 0.49)
        .unwrap();

    // One order was partially filled but still stays on the orderbook
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2) as usize);

    assert_eq!(
        astroport.pool_balances().unwrap().assets,
        [
            astroport.assets[&test_coins[1]].with_balance(500_504_092039u128),
            astroport.assets[&test_coins[0]].with_balance(999_002_684480u128),
        ]
    );

    // Astroport swap NTRN -> ASTRO
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1_000_000000u128);
    let resp = astroport.swap_max_spread(&user, &swap_asset).unwrap();

    // Ensure that the previous order from DEX was processed
    let cumulative_trade: CumulativeTradeUint = resp
        .events
        .iter()
        .flat_map(|e| e.attributes.iter())
        .find_map(|attr| {
            if attr.key == "cumulative_trade" {
                Some(from_str(&attr.value).unwrap())
            } else {
                None
            }
        })
        .unwrap();

    assert_eq!(
        cumulative_trade,
        CumulativeTradeUint {
            base_asset: astroport.assets[&test_coins[0]].with_balance(999_999998u128),
            quote_asset: astroport.assets[&test_coins[1]].with_balance(495_907961u128)
        }
    );

    // DEX swap ASTRO -> NTRN
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(500_000000u128, "astro"), "untrn", 1.9)
        .unwrap();

    assert_eq!(
        astroport.pool_balances().unwrap().assets,
        [
            astroport.assets[&test_coins[1]].with_balance(500_504_089034u128),
            astroport.assets[&test_coins[0]].with_balance(999_012_319119u128),
        ]
    );

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(500_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    let config = astroport.query_config().unwrap();
    assert_eq!(
        config.pool_state.price_state.last_price,
        Decimal256::from_str("1.997288637334109118").unwrap()
    );

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2) as usize);

    // Creating a huge limit order which should be partially consumed by Astroport pair
    let whale = astroport
        .helper
        .app
        .init_account(&[coin(2_000_000_000000u128, "untrn")])
        .unwrap();
    let tranche_key = astroport
        .helper
        .limit_order(&whale, coin(1_000_000_000000u128, "untrn"), "astro", 0.3)
        .unwrap()
        .data
        .tranche_key;

    astroport.sync_orders(&astroport.helper.signer).unwrap();

    let config = astroport.query_config().unwrap();
    assert_eq!(
        config.pool_state.price_state.last_price,
        Decimal256::from_str("2.016503220355952024").unwrap()
    );

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2 - 1) as usize);

    // Astroport swap NTRN -> ASTRO
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1000_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    // Still only 1 order because other side being constantly consumed by whale's order
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2 - 1) as usize);

    // Whale cancels order
    astroport.helper.cancel_order(&whale, &tranche_key).unwrap();

    // let res = astroport
    //     .helper
    //     .dex
    //     .tick_liquidity_all(&QueryAllTickLiquidityRequest {
    //         pair_id: "untrn<>astro".to_string(),
    //         token_in: "untrn".to_string(),
    //         pagination: None,
    //     })
    //     .unwrap();
    // dbg!(res);
    // dbg!(astroport.helper.list_orders(&whale.address()).unwrap());
    // dbg!(astroport
    //     .helper
    //     .list_orders(astroport.pair_addr.as_str())
    //     .unwrap());
    // dbg!(astroport.helper.list_orders(&dex_trader.address()).unwrap());

    // Swap to trigger new orders placement
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    // Confirm we have all orders back
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2) as usize);

    // Ensure that the main LP can withdraw all liquidity
    let lp_token = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap();
    astroport.withdraw_liquidity(&user, lp_token).unwrap();

    // Confirm we no longer have any orders
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), 0);
}

#[test]
fn test_different_decimals() {
    let test_coins = vec![
        TestCoin::native_precise("aeth", 18),
        TestCoin::native("astro"),
    ];
    let orders_number = 1;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
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
            liquidity_percent: Decimal::percent(5),
            orders_number,
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
            coin(1000000_000000u128, "untrn"), // for gas fees
            coin(1_100_000u128 * 1e18 as u128, "aeth"),
            coin(505_000_000000u128, "astro"),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[0]].with_balance(1_000_000u128 * 1e18 as u128),
        astroport.assets[&test_coins[1]].with_balance(500_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    assert_eq!(astroport.pool_balances().unwrap().assets, initial_balances);

    // Astroport swap ASTRO -> ETH
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(1_000_000000u128);
    astroport.swap(&user, &swap_asset, None).unwrap();

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(1000000_000000u128, "untrn"), // for gas fees
            coin(10_000u128 * 1e18 as u128, "aeth"),
        ])
        .unwrap();

    // DEX swap ETH -> ASTRO
    astroport
        .helper
        .swap_on_dex_precise(
            &dex_trader,
            coin(1_000u128 * 1e18 as u128, "aeth"),
            "astro",
            490000000000000, // 0.49e-12 uastro per aeth
        )
        .unwrap();

    let astro_bal = astroport
        .helper
        .query_balance(&dex_trader.address(), "astro")
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 495_859685);

    let dex_trader2 = astroport
        .helper
        .app
        .init_account(&[
            coin(1000000_000000u128, "untrn"), // for gas fees
            coin(10_000_000000u128, "astro"),
        ])
        .unwrap();

    // DEX swap ASTRO -> ETH
    astroport
        .helper
        .swap_on_dex(
            &dex_trader2,
            coin(1_000_000000u128, "astro"),
            "aeth",
            2018138624033.183, // 2018138624033.183 aeth per astro
        )
        .unwrap();

    let eth_bal = astroport
        .helper
        .query_balance(&dex_trader2.address(), "aeth")
        .unwrap();
    assert_eq!(eth_bal.amount.u128(), 1977_163562505171343978);

    // Ensure that the main LP can withdraw all liquidity
    let lp_token = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap();
    astroport.withdraw_liquidity(&user, lp_token).unwrap();

    // Confirm we no longer have any orders
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), 0);
}

#[test]
fn test_sync_after_whole_side_consumed() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("untrn")];
    let orders_number = 5;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        neutron,
        test_coins.clone(),
        ConcentratedPoolParams {
            price_scale: Decimal::from_ratio(1u8, 2u8),
            ..common_pcl_params()
        },
        OrderbookConfig {
            executor: Some(owner),
            liquidity_percent: Decimal::percent(20),
            orders_number,
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
            coin(2_000_000_000000u128, "untrn"),
            coin(2_000_000_000000u128, "astro"),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[0]].with_balance(10_000_000000u128),
        astroport.assets[&test_coins[1]].with_balance(20_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    let ob_liquidity = astroport
        .helper
        .query_total_ob_liquidity(astroport.pair_addr.as_str())
        .unwrap()
        .into_iter()
        .sorted_by(|a, b| a.denom.cmp(&b.denom))
        .collect_vec();
    assert_eq!(
        ob_liquidity,
        [
            coin(1_000_000000u128, "astro"),
            coin(2_000_000000u128, "untrn")
        ]
    );

    let swap_simulation = astroport
        .simulate_swap(astroport.assets[&test_coins[1]].with_balance(2_000_000000u128))
        .unwrap();

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[coin(10_000_000000u128, "untrn")])
        .unwrap();

    // Trying to consume all orders on one side
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(2_000_000000u128, "untrn"), "astro", 0.3)
        .unwrap();

    // Ensure that ASTRO received is less or equal to the simulation
    let astro_bal = astroport
        .helper
        .query_balance(&dex_trader.address(), "astro")
        .unwrap();
    assert!(
        astro_bal.amount <= swap_simulation.return_amount,
        "Received more ASTRO than in simulation: {} <= {}",
        astro_bal.amount,
        swap_simulation.return_amount
    );

    astroport.sync_orders(&astroport.helper.signer).unwrap();
}
