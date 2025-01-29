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
        [coin(12388_891279, "astro"), coin(24_777_782558, "untrn")]
    );

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(1_000_000000u128);
    astroport.swap(&user, &swap_asset, None).unwrap();

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
            astroport.assets[&test_coins[1]].with_balance(500503_744799u128),
            astroport.assets[&test_coins[0]].with_balance(999002_684480u128),
        ]
    );

    // Astroport swap NTRN -> ASTRO
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1_000_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    // DEX swap ASTRO -> NTRN
    astroport
        .helper
        .swap_on_dex(&dex_trader, coin(500_000000u128, "astro"), "untrn", 1.9)
        .unwrap();

    assert_eq!(
        astroport.pool_balances().unwrap().assets,
        [
            astroport.assets[&test_coins[1]].with_balance(500504_388512u128),
            astroport.assets[&test_coins[0]].with_balance(999010_222418u128),
        ]
    );

    // Astroport swap ASTRO -> NTRN
    let swap_asset = astroport.assets[&test_coins[1]].with_balance(500_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

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

    // Swap to trigger new orders placement
    let swap_asset = astroport.assets[&test_coins[0]].with_balance(1_000000u128);
    astroport.swap_max_spread(&user, &swap_asset).unwrap();

    // Confirm we have all orders back
    let orders = astroport
        .helper
        .list_orders(astroport.pair_addr.as_str())
        .unwrap();
    assert_eq!(orders.limit_orders.len(), (orders_number * 2) as usize);
}
