#![cfg(not(tarpaulin_include))]
#![cfg(feature = "test-tube")]
#![allow(dead_code)]

use astroport::asset::AssetInfoExt;
use cosmwasm_std::{coin, Decimal, Uint128};
use neutron_test_tube::{Account, NeutronTestApp};

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
        common_pcl_params(),
        OrderbookConfig {
            enable: true,
            executor: Some(owner),
            liquidity_percent: Decimal::percent(20),
            orders_number: 5,
            min_asset_0_order_size: Uint128::from(1_000u128),
            min_asset_1_order_size: Uint128::from(1_000u128),
        },
    )
    .unwrap();

    let user = astroport
        .helper
        .app
        .init_account(&[
            coin(10_000_0000_000000, "untrn"),
            coin(1_000_0000_000000, "astro"),
        ])
        .unwrap();

    astroport
        .provide_liquidity(
            &user,
            &[
                astroport.assets[&test_coins[0]].with_balance(1_000_000_000000u128),
                astroport.assets[&test_coins[1]].with_balance(1_000_000_000000u128),
            ],
        )
        .unwrap();
}
