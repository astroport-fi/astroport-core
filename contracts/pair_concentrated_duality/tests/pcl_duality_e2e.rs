#![cfg(not(tarpaulin_include))]
#![cfg(feature = "test-tube")]
#![allow(dead_code)]

use std::str::FromStr;

use cosmwasm_schema::schemars::_serde_json::from_str;
use cosmwasm_std::{coin, Decimal, Decimal256, Uint128};
use itertools::Itertools;
use neutron_test_tube::{Account, NeutronTestApp};

use astroport::asset::{Asset, AssetInfoExt};
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
        &neutron,
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
            astroport.assets[&test_coins[1]].with_balance(500_503_619978u128),
            astroport.assets[&test_coins[0]].with_balance(999_012_319119u128),
        ]
    );

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(500_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    let config = astroport.query_config().unwrap();
    assert_eq!(
        config.pool_state.price_state.last_price,
        Decimal256::from_str("1.997288637250696996").unwrap()
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

    let ob_config = astroport.query_ob_config().unwrap();
    assert_eq!(
        ob_config.pre_reply_balances,
        [
            astroport.assets[&test_coins[0]].with_balance(1_023_268_331806u128),
            astroport.assets[&test_coins[1]].with_balance(488_450_348025u128),
        ]
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
        &neutron,
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
        &neutron,
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

#[test]
fn estimate_gas_usage() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("untrn")];
    let orders_number = 15;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        &neutron,
        test_coins.clone(),
        common_pcl_params(),
        OrderbookConfig {
            executor: Some(owner),
            liquidity_percent: Decimal::percent(50),
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
        astroport.assets[&test_coins[1]].with_balance(10_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    // Provide again for clear experiment
    // (includes fetching orderbook liquidity and cancelling old orders)
    let resp = astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();
    println!("Provide gas {:?}", resp.gas_info);

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(100_000000u128);
    let resp = astroport.swap(&user, &swap_asset, None).unwrap();
    println!("Swap gas {:?}", resp.gas_info);

    // Withdraw liquidity
    let mut lp_token = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap();
    // Withdraw only 1%
    lp_token.amount = lp_token.amount.multiply_ratio(1u8, 100u8);
    let resp = astroport.withdraw_liquidity(&user, lp_token).unwrap();
    print!("Withdraw gas {:?}", resp.gas_info);
}

#[test]
fn test_simulation_matches_execution() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("usdc")];
    let orders_number = 5;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        &neutron,
        test_coins.clone(),
        common_pcl_params(),
        OrderbookConfig {
            executor: Some(owner),
            liquidity_percent: Decimal::percent(50),
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
            coin(2_000_000_000000u128, "usdc"),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[0]].with_balance(10_000_000000u128),
        astroport.assets[&test_coins[1]].with_balance(10_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
            coin(10_000_000000u128, "usdc"),
        ])
        .unwrap();

    // Confirm provide simulation matches execution

    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(2_500_000000u128, "usdc"), "astro", 0.5)
        .unwrap();

    let lp_bal_before = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap()
        .amount;
    let simulated_amount = astroport
        .simulate_provide_liquidity(initial_balances.to_vec())
        .unwrap();

    // Astroport provide liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    let lp_amount = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap()
        .amount
        - lp_bal_before;

    assert_eq!(lp_amount, simulated_amount);

    // Confirm swap simulation matches real execution result

    // DEX swap USDC -> ASTRO
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(1_000_000000u128, "usdc"), "astro", 0.5)
        .unwrap();

    let astro_bal_before = astroport
        .helper
        .query_balance(&user.address(), "astro")
        .unwrap()
        .amount;
    let simulation = astroport
        .simulate_swap(astroport.assets[&test_coins[1]].with_balance(100_000000u128))
        .unwrap();

    // Astroport swap USDC -> ASTRO
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(100_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    let astro_swap_amount = astroport
        .helper
        .query_balance(&user.address(), "astro")
        .unwrap()
        .amount
        - astro_bal_before;

    // Because PCL repeg algo is time-dependent, the swap amount can be slightly different.
    // Query simulation uses old block time while swap pushes block production on test-tube hence
    // incrementing block time by 3 seconds
    assert_eq!(astro_swap_amount.u128(), 73_687816);
    assert_eq!(simulation.return_amount.u128(), 73_687755);

    // Confirm withdraw simulation matches execution result

    // DEX swap USDC -> ASTRO
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(1_000_000000u128, "usdc"), "astro", 0.5)
        .unwrap();

    let astro_bal_before = astroport
        .helper
        .query_balance(&user.address(), "astro")
        .unwrap()
        .amount;
    let usdc_bal_before = astroport
        .helper
        .query_balance(&user.address(), "usdc")
        .unwrap()
        .amount;

    let mut lp_coins = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap();
    // Withdraw half
    lp_coins.amount = lp_coins.amount.multiply_ratio(1u8, 2u8);
    let simulation = astroport
        .simulate_withdraw_liquidity(lp_coins.amount)
        .unwrap();

    // Astroport withdraw liquidity
    astroport.withdraw_liquidity(&user, lp_coins).unwrap();

    let astro_withdrawn = astroport
        .helper
        .query_balance(&user.address(), "astro")
        .unwrap()
        .amount
        - astro_bal_before;
    let usdc_withdrawn = astroport
        .helper
        .query_balance(&user.address(), "usdc")
        .unwrap()
        .amount
        - usdc_bal_before;

    assert_eq!(
        simulation,
        [
            Asset::native("astro", astro_withdrawn),
            Asset::native("usdc", usdc_withdrawn)
        ]
    );
}

#[test]
fn test_cumulative_trade_when_both_sides_filled() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("usdc")];
    let orders_number = 5;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        &neutron,
        test_coins.clone(),
        common_pcl_params(),
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
            coin(2_000_000_000000u128, "usdc"),
        ])
        .unwrap();

    let initial_balances = [
        astroport.assets[&test_coins[0]].with_balance(10_000_000000u128),
        astroport.assets[&test_coins[1]].with_balance(10_000_000000u128),
    ];

    // Providing initial liquidity
    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
            coin(10_000_000000u128, "usdc"),
        ])
        .unwrap();

    // DEX swap USDC -> ASTRO
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(1_000_000000u128, "usdc"), "astro", 0.3)
        .unwrap();

    // DEX swap ASTRO -> USDC
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(700_000000u128, "astro"), "usdc", 0.5)
        .unwrap();

    // Sync with orderbook
    let resp = astroport.sync_orders(&astroport.helper.signer).unwrap();

    // Check cumulative trades
    let cumulative_trades: Vec<CumulativeTradeUint> = resp
        .events
        .iter()
        .flat_map(|e| e.attributes.iter())
        .filter_map(|attr| {
            if attr.key == "cumulative_trade" {
                Some(from_str(&attr.value).unwrap())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        cumulative_trades,
        [
            CumulativeTradeUint {
                base_asset: astroport.assets[&test_coins[0]].with_balance(699_999996u128),
                quote_asset: astroport.assets[&test_coins[1]].with_balance(665_347201u128)
            },
            CumulativeTradeUint {
                base_asset: astroport.assets[&test_coins[1]].with_balance(999_999995u128),
                quote_asset: astroport.assets[&test_coins[0]].with_balance(923_642189u128)
            }
        ]
    );

    let config = astroport.query_config().unwrap();
    assert_eq!(
        config.pool_state.price_state.last_price.to_string(),
        "0.668281266463892125"
    );

    // DEX swap USDC -> ASTRO
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(10u128, "usdc"), "astro", 0.3)
        .unwrap();

    // DEX swap ASTRO -> USDC
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(700_000000u128, "astro"), "usdc", 0.4)
        .unwrap();

    // Sync with orderbook
    let resp = astroport.sync_orders(&astroport.helper.signer).unwrap();

    // Check cumulative trades
    let cumulative_trades: Vec<CumulativeTradeUint> = resp
        .events
        .iter()
        .flat_map(|e| e.attributes.iter())
        .filter_map(|attr| {
            if attr.key == "cumulative_trade" {
                Some(from_str(&attr.value).unwrap())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        cumulative_trades,
        [
            CumulativeTradeUint {
                base_asset: astroport.assets[&test_coins[0]].with_balance(699999996u128),
                quote_asset: astroport.assets[&test_coins[1]].with_balance(690075060u128)
            },
            CumulativeTradeUint {
                base_asset: astroport.assets[&test_coins[1]].with_balance(9u128),
                quote_asset: astroport.assets[&test_coins[0]].with_balance(10u128)
            }
        ]
    );
    // We've received 699999996 - 10 astro and sold 690075060 - 9 usdc =>
    // (699999996 - 10) / (690075060 - 9) = 1.014382399400786

    let config = astroport.query_config().unwrap();
    assert_eq!(
        config.pool_state.price_state.last_price.to_string(),
        "1.014382399400786335"
    );
}

#[test]
fn check_orderbook_after_withdrawal() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("usdc")];
    let orders_number = 5;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        &neutron,
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
            coin(20_000_000_000000u128, "astro"),
            coin(20_000_000_000000u128, "usdc"),
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

    // Withdraw half
    let mut lp_coins = astroport
        .helper
        .query_balance(&user.address(), &astroport.lp_token)
        .unwrap();
    lp_coins.amount = lp_coins.amount.multiply_ratio(1u8, 2u8);
    astroport.withdraw_liquidity(&user, lp_coins).unwrap();

    let trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
        ])
        .unwrap();

    // Swap on DEX a little liquidity to make sure the price there is close to the simulation
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(10_000000u128);
    let return_amount = astroport
        .simulate_swap(swap_asset.clone())
        .unwrap()
        .return_amount;
    let price = return_amount.u128() as f64 / 10_000000u128 as f64;
    // Worsen price by 5%
    let price = price * 0.95;
    astroport
        .helper
        .swap_on_dex(&trader, swap_asset.as_coin().unwrap(), "usdc", price)
        .unwrap();

    let usdc_received = astroport
        .helper
        .query_balance(&trader.address(), "usdc")
        .unwrap()
        .amount
        .u128() as f64;
    let usdc_expected = 10_000000.0 * price;

    assert!(
        (1.0 - usdc_expected / usdc_received).abs() <= 0.05,
        "Expected: {usdc_expected}, received: {usdc_received}",
    );
}

#[test]
fn check_partial_auto_executed_order() {
    let test_coins = vec![TestCoin::native("astro"), TestCoin::native("usdc")];
    let orders_number = 3;

    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        &neutron,
        test_coins.clone(),
        ConcentratedPoolParams {
            price_scale: Decimal::from_ratio(1u8, 2u8),
            ..common_pcl_params()
        },
        OrderbookConfig {
            executor: Some(owner),
            liquidity_percent: Decimal::percent(3),
            orders_number,
            min_asset_0_order_size: Uint128::from(1_000u128),
            min_asset_1_order_size: Uint128::from(1_000u128),
            avg_price_adjustment: Decimal::from_str("0.0001").unwrap(),
        },
    )
    .unwrap();

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
            coin(10_000_000000u128, "usdc"),
        ])
        .unwrap();

    // Create limit order on DEX: ASTRO -> USDC
    astroport
        .helper
        .limit_order(&dex_trader, coin(1_000_000000u128, "astro"), "usdc", 0.49)
        .unwrap();

    astroport.enable_orderbook(&astroport.owner, true).unwrap();

    let user = astroport
        .helper
        .app
        .init_account(&[
            coin(2_000_000_000000u128, "untrn"),
            coin(20_000_000_000000u128, "astro"),
            coin(20_000_000_000000u128, "usdc"),
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

    let ob_config = astroport.query_ob_config().unwrap();

    // Ensure that the previous order from DEX was processed
    assert_eq!(
        ob_config.delayed_trade,
        Some(CumulativeTradeUint {
            base_asset: Asset::native("astro", 1_000_000000u128),
            quote_asset: Asset::native("usdc", 490_041922u128)
        })
    );
}
