#![cfg(not(tarpaulin_include))]
#![cfg(feature = "test-tube")]
#![allow(dead_code)]

use cosmwasm_std::{coin, Decimal, Uint128};
use itertools::Itertools;
use neutron_test_tube::cosmrs::proto::cosmos::bank::v1beta1::QueryAllBalancesRequest;
use neutron_test_tube::{Account, NeutronTestApp};

use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::pair_concentrated_duality::OrderbookConfig;
use astroport_test::coins::TestCoin;
use common::{
    astroport_wrapper::AstroportHelper, helper::common_pcl_params, neutron_wrapper::TestAppWrapper,
};

mod common;

#[test]
fn init_on_duality() {
    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("astro")];
    let app = NeutronTestApp::new();
    let neutron = TestAppWrapper::bootstrap(&app).unwrap();
    let owner = neutron.signer.address();

    let astroport = AstroportHelper::new(
        neutron,
        test_coins.clone(),
        ConcentratedPoolParams {
            price_scale: Decimal::percent(50),
            ..common_pcl_params()
        },
        OrderbookConfig {
            enable: true,
            executor: Some(owner),
            liquidity_percent: Decimal::percent(5),
            orders_number: 1,
            min_asset_0_order_size: Uint128::from(1_000u128),
            min_asset_1_order_size: Uint128::from(1_000u128),
        },
    )
    .unwrap();

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

    astroport
        .provide_liquidity(&user, &initial_balances)
        .unwrap();

    let balances = astroport
        .pool_balances()
        .unwrap()
        .assets
        .into_iter()
        .sorted_by(|a, b| a.info.to_string().cmp(&b.info.to_string()))
        .collect_vec();

    assert_eq!(balances, initial_balances);

    // let orders = astroport
    //     .helper
    //     .list_orders(astroport.pair_addr.as_str())
    //     .unwrap()
    //     .limit_orders
    //     .into_iter()
    //     .map(|order| order.tranche_key)
    //     .collect_vec();
    // dbg!(orders);
    // let total_liquidity = astroport
    //     .helper
    //     .query_total_ob_liquidity(astroport.pair_addr.as_str())
    //     .unwrap();
    // dbg!(total_liquidity);

    let swap_asset = astroport.assets[&test_coins[1]].with_balance(1_000_000000u128);
    astroport.swap(&user, &swap_asset, None).unwrap();

    dbg!(astroport.pool_balances().unwrap());

    // let orders = astroport
    //     .helper
    //     .list_orders(astroport.pair_addr.as_str())
    //     .unwrap()
    //     .limit_orders
    //     .into_iter()
    //     .map(|order| order.tranche_key)
    //     .collect_vec();
    // dbg!(orders);

    let dex_trader = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_000000u128, "untrn"),
            coin(10_000_000000u128, "astro"),
        ])
        .unwrap();

    // let orders = astroport
    //     .helper
    //     .list_orders(astroport.pair_addr.as_str())
    //     .unwrap();
    // dbg!(orders);

    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(1_000_000000u128, "untrn"), "astro", 0.49)
        .unwrap();

    // let bal = astroport
    //     .helper
    //     .bank
    //     .query_all_balances(&QueryAllBalancesRequest {
    //         address: dex_trader.address(),
    //         pagination: None,
    //     })
    //     .unwrap();
    // dbg!(bal);

    dbg!(astroport.pool_balances().unwrap());

    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1_000_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(500_000000u128, "astro"), "untrn", 1.9)
        .unwrap();

    let swap_asset = astroport.assets[&test_coins[1]].with_balance(500_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    dbg!(astroport.pool_balances().unwrap());

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    dbg!(orders);

    // Creating a huge limit order which should be partially consumed by Astroport pair
    let whale = astroport
        .helper
        .app
        .init_account(&[coin(2_000_000_000000u128, "untrn")])
        .unwrap();
    astroport
        .helper
        .limit_order(&whale, coin(1_000_000_000000u128, "untrn"), "astro", 0.3)
        .unwrap();

    astroport.sync_orders(&astroport.helper.signer).unwrap();

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    dbg!(orders);

    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1000_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    dbg!(orders);

    panic!("Test panic")
}
