#![cfg(not(tarpaulin_include))]

use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal, Decimal256, StdError, Uint128};
use cw2::set_contract_version;
use itertools::{max, Itertools};

use astroport::asset::{native_asset_info, Asset, AssetInfoExt, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::factory::PairType;
use astroport::pair::{QueryMsg, MAX_FEE_SHARE_BPS};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, PromoteParams, UpdatePoolParams,
};
use astroport::pair_concentrated_duality::{DualityPairMsg, MigrateMsg, OrderbookConfig};
use astroport_pair_concentrated_duality::error::ContractError;
use astroport_pair_concentrated_duality::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use astroport_pair_concentrated_duality::orderbook::error::OrderbookError;
use astroport_pcl_common::consts::{AMP_MAX, AMP_MIN, MA_HALF_TIME_LIMITS};
use astroport_pcl_common::error::PclError;
use astroport_test::coins::TestCoin;
use astroport_test::convert::{dec_to_f64, f64_to_dec};
use astroport_test::cw_multi_test::Executor;

use crate::common::helper::{common_pcl_params, pcl_duality_contract, ExecuteMsg, Helper};

mod common;

#[test]
fn check_wrong_initialization() {
    let owner = Addr::unchecked("owner");

    let params = common_pcl_params();

    let mut wrong_params = params.clone();
    wrong_params.amp = Decimal::zero();

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("untrn"), TestCoin::native("ASTRO")],
        wrong_params,
        true,
    )
    .unwrap_err();

    assert_eq!(
        ContractError::PclError(PclError::IncorrectPoolParam(
            "amp".to_string(),
            AMP_MIN.to_string(),
            AMP_MAX.to_string()
        )),
        err.downcast().unwrap(),
    );

    let mut wrong_params = params.clone();
    wrong_params.ma_half_time = MA_HALF_TIME_LIMITS.end() + 1;

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("untrn"), TestCoin::native("ASTRO")],
        wrong_params,
        true,
    )
    .unwrap_err();

    assert_eq!(
        ContractError::PclError(PclError::IncorrectPoolParam(
            "ma_half_time".to_string(),
            MA_HALF_TIME_LIMITS.start().to_string(),
            MA_HALF_TIME_LIMITS.end().to_string()
        )),
        err.downcast().unwrap(),
    );

    let mut wrong_params = params.clone();
    wrong_params.price_scale = Decimal::zero();

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("untrn"), TestCoin::native("ASTRO")],
        wrong_params,
        true,
    )
    .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Initial price scale can not be zero",
    );

    // check instantiation with valid params
    Helper::new(
        &owner,
        vec![TestCoin::native("untrn"), TestCoin::native("ASTRO")],
        params,
        true,
    )
    .unwrap();
}

#[test]
fn check_create_pair_with_unsupported_denom() {
    let owner = Addr::unchecked("owner");

    let wrong_coins = vec![TestCoin::native("rc"), TestCoin::native("uusdc")];
    let valid_coins = vec![TestCoin::native("uluna"), TestCoin::native("uusdc")];

    let params = common_pcl_params();

    let err = Helper::new(&owner, wrong_coins.clone(), params.clone(), true).unwrap_err();
    assert_eq!(
        "Generic error: Invalid denom length [3,128]: rc",
        err.root_cause().to_string()
    );

    Helper::new(&owner, valid_coins.clone(), params.clone(), true).unwrap();
}

#[test]
fn provide_and_withdraw() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::native("uusdc")];

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    // checking LP token virtual price on an empty pool
    let lp_price = helper.query_lp_price().unwrap();
    assert!(
        lp_price.is_zero(),
        "LP price must be zero before any provide"
    );

    let user1 = Addr::unchecked("user1");

    let random_coin = native_asset_info("random-coin".to_string()).with_balance(100u8);
    let wrong_assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        random_coin.clone(),
    ];

    helper.give_me_money(&wrong_assets, &user1);

    // Provide with empty assets
    let err = helper.provide_liquidity(&user1, &[]).unwrap_err();
    assert_eq!(
        "Generic error: Nothing to provide",
        err.root_cause().to_string()
    );

    // Provide just one asset which does not belong to the pair
    let err = helper
        .provide_liquidity(&user1, &[random_coin.clone()])
        .unwrap_err();
    assert_eq!(
        "The asset random-coin does not belong to the pair",
        err.root_cause().to_string()
    );

    helper.give_me_money(&[helper.assets[&test_coins[1]].with_balance(1u8)], &user1);

    // Try to provide 3 assets
    let err = helper
        .provide_liquidity(
            &user1,
            &[
                random_coin.clone(),
                helper.assets[&test_coins[0]].with_balance(1u8),
                helper.assets[&test_coins[1]].with_balance(1u8),
            ],
        )
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidNumberOfAssets(2),
        err.downcast().unwrap()
    );

    helper.give_me_money(
        &[helper.assets[&test_coins[1]].with_balance(50_000_000000u128 - 1)],
        &user1,
    );

    // Try to provide with zero amount
    let err = helper
        .provide_liquidity(
            &user1,
            &[
                helper.assets[&test_coins[0]].with_balance(0u8),
                helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
            ],
        )
        .unwrap_err();
    assert_eq!(ContractError::InvalidZeroAmount {}, err.downcast().unwrap());

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];

    // Test very small initial provide
    let err = helper
        .provide_liquidity(
            &user1,
            &[
                helper.assets[&test_coins[0]].with_balance(1000u128),
                helper.assets[&test_coins[1]].with_balance(500u128),
            ],
        )
        .unwrap_err();
    assert_eq!(
        ContractError::MinimumLiquidityAmountError {},
        err.downcast().unwrap()
    );

    // This is normal provision
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        70710_677118,
        helper.native_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    assert_eq!(
        helper
            .query_share(helper.native_balance(&helper.lp_token, &user1))
            .unwrap(),
        vec![
            helper.assets[&test_coins[0]].with_balance(99999998584u128),
            helper.assets[&test_coins[1]].with_balance(49999999292u128)
        ]
    );

    let user2 = Addr::unchecked("user2");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];
    helper.give_me_money(&assets, &user2);
    helper.provide_liquidity(&user2, &assets).unwrap();
    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128(),
        helper.native_balance(&helper.lp_token, &user2)
    );

    // Changing order of assets does not matter
    let user3 = Addr::unchecked("user3");
    let assets = vec![
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user3);
    helper.provide_liquidity(&user3, &assets).unwrap();
    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128(),
        helper.native_balance(&helper.lp_token, &user3)
    );

    // After initial provide one-sided provide is allowed
    let user4 = Addr::unchecked("user4");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(0u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user4);
    helper.provide_liquidity(&user4, &assets).unwrap();
    // LP amount is less than for prev users as provide is imbalanced
    assert_eq!(
        62217_722016,
        helper.native_balance(&helper.lp_token, &user4)
    );

    // One of assets may be omitted
    let user5 = Addr::unchecked("user5");
    let assets = vec![helper.assets[&test_coins[0]].with_balance(140_000_000000u128)];
    helper.give_me_money(&assets, &user5);
    helper.provide_liquidity(&user5, &assets).unwrap();
    assert_eq!(
        57271_023590,
        helper.native_balance(&helper.lp_token, &user5)
    );

    // check that imbalanced withdraw is currently disabled
    let withdraw_assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(5_000_000000u128),
    ];
    let err = helper
        .withdraw_liquidity(&user1, 7071_067711, withdraw_assets)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Imbalanced withdraw is currently disabled"
    );

    // user1 withdraws 1/10 of his LP tokens
    helper
        .withdraw_liquidity(&user1, 7071_067711, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 - 7071_067711,
        helper.native_balance(&helper.lp_token, &user1)
    );
    assert_eq!(9382_010960, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(5330_688045, helper.coin_balance(&test_coins[1], &user1));

    // user2 withdraws half
    helper
        .withdraw_liquidity(&user2, 35355_339059, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128() - 35355_339059,
        helper.native_balance(&helper.lp_token, &user2)
    );
    assert_eq!(46910_055478, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(26653_440612, helper.coin_balance(&test_coins[1], &user2));

    let err = helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![helper.assets[&test_coins[0]].with_balance(0u128)]),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::WrongAssetLength {
            expected: 2,
            actual: 1
        },
        err.downcast().unwrap()
    );

    let err = helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![
                Asset::native("random", 100u128),
                Asset::native("random2", 100u128),
            ]),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidAsset("random".to_string()),
        err.downcast().unwrap()
    );

    let err = helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![
                helper.assets[&test_coins[0]].with_balance(0u128),
                helper.assets[&test_coins[0]].with_balance(0u128),
            ]),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Duplicated assets in min_assets_to_receive"
    );

    let err = helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![
                helper.assets[&test_coins[1]].with_balance(100_000_0000000u128),
                helper.assets[&test_coins[0]].with_balance(0u128),
            ]),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::WithdrawSlippageViolation {
            asset_name: helper.assets[&test_coins[1]].to_string(),
            received: 7538731439u128.into(),
            expected: 100_000_0000000u128.into(),
        },
        err.downcast().unwrap()
    );

    let err = helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![
                helper.assets[&test_coins[1]].with_balance(7538731439u128),
                helper.assets[&test_coins[0]].with_balance(100_000_0000000u128),
            ]),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::WithdrawSlippageViolation {
            asset_name: helper.assets[&test_coins[0]].to_string(),
            received: 13268167332u128.into(),
            expected: 100_000_0000000u128.into(),
        },
        err.downcast().unwrap()
    );

    helper
        .withdraw_liquidity_full(
            &user2,
            10_000_000000,
            vec![],
            Some(vec![
                helper.assets[&test_coins[1]].with_balance(7538731439u128),
                helper.assets[&test_coins[0]].with_balance(13268167332u128),
            ]),
        )
        .unwrap();
}

#[test]
fn check_imbalanced_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params.clone(), true).unwrap();

    let user1 = Addr::unchecked("user1");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        100285_256937,
        helper.native_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    // creating a new pool with inverted price scale
    params.price_scale = Decimal::from_ratio(1u8, 2u8);

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        100285_256937,
        helper.native_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));
}

#[test]
fn provide_with_different_precision() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native_precise("foo", 5),
        TestCoin::native("untrn"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_00000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    let tolerance = 9;

    for user_name in ["user1", "user2", "user3"] {
        let user = Addr::unchecked(user_name);

        helper.give_me_money(&assets, &user);

        helper.provide_liquidity(&user, &assets).unwrap();

        let lp_amount = helper.native_balance(&helper.lp_token, &user);
        assert!(
            100_000000 - lp_amount < tolerance,
            "LP token balance assert failed for {user}"
        );
        assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
        assert_eq!(0, helper.coin_balance(&test_coins[1], &user));

        helper.withdraw_liquidity(&user, lp_amount, vec![]).unwrap();

        assert_eq!(0, helper.native_balance(&helper.lp_token, &user));
        assert!(
            100_00000 - helper.coin_balance(&test_coins[0], &user) < tolerance,
            "Withdrawn amount of coin0 assert failed for {user}"
        );
        assert!(
            100_000000 - helper.coin_balance(&test_coins[1], &user) < tolerance,
            "Withdrawn amount of coin1 assert failed for {user}"
        );
    }
}

#[test]
fn swap_different_precisions() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native_precise("foo", 5),
        TestCoin::native("untrn"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_00000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    let user = Addr::unchecked("user");
    // 100 x FOO tokens
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_00000u128);

    // Checking direct swap simulation
    let sim_resp = helper.simulate_swap(&offer_asset, None).unwrap();
    // And reverse swap as well
    let reverse_sim_resp = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[1]].with_balance(sim_resp.return_amount.u128()),
            None,
        )
        .unwrap();
    assert_eq!(reverse_sim_resp.offer_amount.u128(), 10019003);
    assert_eq!(reverse_sim_resp.commission_amount.u128(), 45084);
    assert_eq!(reverse_sim_resp.spread_amount.u128(), 125);

    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
    // 99_737929 x BAR tokens
    assert_eq!(99_737929, sim_resp.return_amount.u128());
    assert_eq!(
        sim_resp.return_amount.u128(),
        helper.coin_balance(&test_coins[1], &user)
    );
}

#[test]
fn simulate_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::native("untrn")];

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];

    let user1 = Addr::unchecked("user1");

    let shares: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            helper.pair_addr.to_string(),
            &QueryMsg::SimulateProvide {
                assets: assets.clone(),
                slippage_tolerance: None,
            },
        )
        .unwrap();

    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        shares.u128(),
        helper.native_balance(&helper.lp_token, &user1)
    );

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_0000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];

    let err = helper
        .app
        .wrap()
        .query_wasm_smart::<Uint128>(
            helper.pair_addr.to_string(),
            &QueryMsg::SimulateProvide {
                assets: assets.clone(),
                slippage_tolerance: Option::from(Decimal::percent(1)),
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: Operation exceeds max spread limit")
    );
}

#[test]
fn check_reverse_swap() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    let offer_asset = helper.assets[&test_coins[0]].with_balance(50_000_000000u128);

    let sim_resp = helper.simulate_swap(&offer_asset, None).unwrap();
    let reverse_sim_resp = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[1]].with_balance(sim_resp.return_amount.u128()),
            None,
        )
        .unwrap();
    assert_eq!(reverse_sim_resp.offer_amount.u128(), 50000220879u128); // as it is hard to predict dynamic fees reverse swap is not exact
    assert_eq!(reverse_sim_resp.commission_amount.u128(), 151_913981);
    assert_eq!(reverse_sim_resp.spread_amount.u128(), 16241_558397);
}

#[test]
fn check_swaps_simple() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);

    // Check swap does not work if pool is empty
    let err = helper.swap(&user, &offer_asset, None).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: One of the pools is empty"
    );

    // Try to swap a wrong asset
    let wrong_coin = native_asset_info("random-coin".to_string());
    let wrong_asset = wrong_coin.with_balance(100_000000u128);
    helper.give_me_money(&[wrong_asset.clone()], &user);
    let err = helper.swap(&user, &wrong_asset, None).unwrap_err();
    assert_eq!(
        ContractError::InvalidAsset(wrong_coin.to_string()),
        err.downcast().unwrap()
    );

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000f64);

    helper.swap(&user, &offer_asset, None).unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
    assert_eq!(99_737929, helper.coin_balance(&test_coins[1], &user));

    let offer_asset = helper.assets[&test_coins[0]].with_balance(90_000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    let err = helper.swap(&user, &offer_asset, None).unwrap_err();
    assert_eq!(
        ContractError::PclError(PclError::MaxSpreadAssertion {}),
        err.downcast().unwrap()
    );

    let user2 = Addr::unchecked("user2");
    let offer_asset = helper.assets[&test_coins[1]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user2);
    helper.swap(&user2, &offer_asset, None).unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user2));
    assert_eq!(99_741246, helper.coin_balance(&test_coins[0], &user2));

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000.260415);
}

#[test]
fn check_swaps_with_price_update() {
    let owner = Addr::unchecked("owner");
    let half = Decimal::from_ratio(1u8, 2u8);

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    helper.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.next_block(1000);

    let user1 = Addr::unchecked("user1");
    let offer_asset = helper.assets[&test_coins[1]].with_balance(10_000_000000u128);
    let mut prev_vlp_price = helper.query_lp_price().unwrap();

    for i in 0..4 {
        helper.give_me_money(&[offer_asset.clone()], &user1);
        helper.swap(&user1, &offer_asset, Some(half)).unwrap();
        let new_vlp_price = helper.query_lp_price().unwrap();
        assert!(
            new_vlp_price >= prev_vlp_price,
            "{i}: new_vlp_price <= prev_vlp_price ({new_vlp_price} <= {prev_vlp_price})",
        );
        prev_vlp_price = new_vlp_price;
        helper.next_block(1000);
    }

    let offer_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000u128);
    for _i in 0..4 {
        helper.give_me_money(&[offer_asset.clone()], &user1);
        helper.swap(&user1, &offer_asset, Some(half)).unwrap();
        helper.next_block(1000);
    }
}

#[test]
fn provides_and_swaps() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    helper.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.next_block(1000);

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    let provider = Addr::unchecked("provider");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000_000000u128),
    ];
    helper.give_me_money(&assets, &provider);
    helper.provide_liquidity(&provider, &assets).unwrap();

    let offer_asset = helper.assets[&test_coins[1]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    helper
        .withdraw_liquidity(&provider, 999_999354, vec![])
        .unwrap();

    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();
}

#[test]
fn check_amp_gamma_change() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.0001),
        ..common_pcl_params()
    };
    let mut helper = Helper::new(&owner, test_coins, params, true).unwrap();

    let random_user = Addr::unchecked("random");
    let action = ConcentratedPoolUpdateParams::Update(UpdatePoolParams {
        mid_fee: Some(f64_to_dec(0.002)),
        out_fee: None,
        fee_gamma: None,
        repeg_profit_threshold: None,
        min_price_scale_delta: None,
        ma_half_time: None,
        allowed_xcp_profit_drop: None,
        xcp_profit_losses_threshold: None,
    });

    let err = helper.update_config(&random_user, &action).unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    helper.update_config(&owner, &action).unwrap();

    helper.next_block(86400);

    let future_time = helper.app.block_info().time.seconds() + 100_000;
    let target_amp = 44f64;
    let target_gamma = 0.00009;
    let action = ConcentratedPoolUpdateParams::Promote(PromoteParams {
        next_amp: f64_to_dec(target_amp),
        next_gamma: f64_to_dec(target_gamma),
        future_time,
    });
    helper.update_config(&owner, &action).unwrap();

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 40f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.0001);
    assert_eq!(amp_gamma.future_time, future_time);

    helper.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 42f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.000095);
    assert_eq!(amp_gamma.future_time, future_time);

    helper.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), target_amp);
    assert_eq!(dec_to_f64(amp_gamma.gamma), target_gamma);
    assert_eq!(amp_gamma.future_time, future_time);

    // change values back
    let future_time = helper.app.block_info().time.seconds() + 100_000;
    let action = ConcentratedPoolUpdateParams::Promote(PromoteParams {
        next_amp: f64_to_dec(40f64),
        next_gamma: f64_to_dec(0.000099),
        future_time,
    });
    helper.update_config(&owner, &action).unwrap();

    helper.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 42f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.0000945);
    assert_eq!(amp_gamma.future_time, future_time);

    // stop changing amp and gamma thus fixing current values
    let action = ConcentratedPoolUpdateParams::StopChangingAmpGamma {};
    helper.update_config(&owner, &action).unwrap();
    let amp_gamma = helper.query_amp_gamma().unwrap();
    let last_change_time = helper.app.block_info().time.seconds();
    assert_eq!(amp_gamma.future_time, last_change_time);

    helper.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 42f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.0000945);
    assert_eq!(amp_gamma.future_time, last_change_time);
}

#[test]
fn check_prices() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("usdx")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();
    helper.next_block(50_000);

    let check_prices = |helper: &Helper| {
        let prices = helper.query_prices().unwrap();

        test_coins
            .iter()
            .cartesian_product(test_coins.iter())
            .filter(|(a, b)| a != b)
            .for_each(|(from_coin, to_coin)| {
                let price = prices
                    .cumulative_prices
                    .iter()
                    .filter(|(from, to, _)| {
                        from.eq(&helper.assets[from_coin]) && to.eq(&helper.assets[to_coin])
                    })
                    .collect::<Vec<_>>();
                assert_eq!(price.len(), 1);
                assert!(!price[0].2.is_zero());
            });
    };

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();
    check_prices(&helper);

    helper.next_block(1000);

    let user1 = Addr::unchecked("user1");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(1000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);

    helper.swap(&user1, &offer_asset, None).unwrap();
    check_prices(&helper);

    helper.next_block(86400);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&assets, &user1);

    helper.provide_liquidity(&user1, &assets).unwrap();
    check_prices(&helper);

    helper.next_block(14 * 86400);

    let offer_asset = helper.assets[&test_coins[1]].with_balance(10_000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);
    helper.swap(&user1, &offer_asset, None).unwrap();
    check_prices(&helper);
}

#[test]
fn update_owner() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("uusd")];

    let mut helper = Helper::new(&owner, test_coins, common_pcl_params(), true).unwrap();

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.pair_addr.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.pair_addr.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    helper
        .app
        .execute_contract(
            Addr::unchecked(&helper.owner),
            helper.pair_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.pair_addr.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.pair_addr.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.pair_addr.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Propose new owner
    helper
        .app
        .execute_contract(
            Addr::unchecked(&helper.owner),
            helper.pair_addr.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // Claim ownership
    helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.pair_addr.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap();

    let config = helper.query_config().unwrap();
    assert_eq!(config.owner.unwrap().to_string(), new_owner)
}

#[test]
fn query_d_test() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("untrn")];

    // create pair with test_coins
    let helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    // query current pool D value before providing any liquidity
    let err = helper.query_d().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Generic error: Pools are empty"
    );
}

#[test]
fn test_ob_integration() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("untrn")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1_000_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(500_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.enable_orderbook(true).unwrap();

    let err = helper
        .app
        .execute_contract(
            owner,
            helper.pair_addr.clone(),
            &ExecuteMsg::Custom(DualityPairMsg::SyncOrderbook {}),
            &[],
        )
        .unwrap_err();

    assert_eq!(
        ContractError::OrderbookError(OrderbookError::NoNeedToSync {}),
        err.downcast().unwrap()
    );

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    let ob_config = helper.query_ob_config().unwrap();

    assert_eq!(ob_config.orders.len() as u8, ob_config.orders_number * 2);
}

#[test]
fn provide_withdraw_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("untrn")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(10u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_938039u128),
        helper.assets[&test_coins[1]].with_balance(1_093804u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();
    helper.next_block(90);
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.next_block(90);
    let uusd = helper.assets[&test_coins[0]].with_balance(5_000000u128);
    helper.give_me_money(&[uusd.clone()], &owner);
    helper.swap(&owner, &uusd, Some(f64_to_dec(0.5))).unwrap();

    helper.next_block(600);
    // Withdraw all
    let lp_amount = helper.native_balance(&helper.lp_token, &owner);
    helper
        .withdraw_liquidity(&owner, lp_amount, vec![])
        .unwrap();

    // Provide again
    helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.5)))
        .unwrap();
}

#[test]
fn provide_withdraw_slippage() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("untrn")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(10u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    // Fully balanced provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.02)))
        .unwrap();

    // Imbalanced provide. Slippage is more than 2% while we enforce 2% max slippage
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(5_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    let err = helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.02)))
        .unwrap_err();
    assert_eq!(
        ContractError::PclError(PclError::MaxSpreadAssertion {}),
        err.downcast().unwrap(),
    );
    // With 3% slippage it should work
    helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.03)))
        .unwrap();

    // Provide with a huge imbalance. Slippage is ~42.2%
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1000_000000u128),
        helper.assets[&test_coins[1]].with_balance(1000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    let err = helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.02)))
        .unwrap_err();
    assert_eq!(
        ContractError::PclError(PclError::MaxSpreadAssertion {}),
        err.downcast().unwrap(),
    );
    helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.5)))
        .unwrap();

    helper.give_me_money(&assets, &owner);
    let err = helper
        .provide_liquidity_full(
            &owner,
            &assets,
            Some(f64_to_dec(0.5)),
            None,
            None,
            Some(10000000000u128.into()),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::ProvideSlippageViolation(1000229863u128.into(), 10000000000u128.into()),
        err.downcast().unwrap(),
    );

    helper
        .provide_liquidity_full(
            &owner,
            &assets,
            Some(f64_to_dec(0.5)),
            None,
            None,
            Some(1000229863u128.into()),
        )
        .unwrap();
}

#[test]
fn check_correct_fee_share() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::native("uusdc")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params(), true).unwrap();

    let share_recipient = Addr::unchecked("share_recipient");
    // Attempt setting fee share with max+1 fee share
    let action = ConcentratedPoolUpdateParams::EnableFeeShare {
        fee_share_bps: MAX_FEE_SHARE_BPS + 1,
        fee_share_address: share_recipient.to_string(),
    };
    let err = helper.update_config(&owner, &action).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::FeeShareOutOfBounds {}
    );

    let action = ConcentratedPoolUpdateParams::EnableFeeShare {
        fee_share_bps: 0,
        fee_share_address: share_recipient.to_string(),
    };
    let err = helper.update_config(&owner, &action).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::FeeShareOutOfBounds {}
    );

    helper.next_block(1000);

    // Set to 5% fee share
    let action = ConcentratedPoolUpdateParams::EnableFeeShare {
        fee_share_bps: 1000,
        fee_share_address: share_recipient.to_string(),
    };
    helper.update_config(&owner, &action).unwrap();

    let config = helper.query_config().unwrap();
    let fee_share = config.fee_share.unwrap();
    assert_eq!(fee_share.bps, 1000u16);
    assert_eq!(fee_share.recipient, share_recipient.to_string());

    helper.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &owner);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.next_block(1000);

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    let last_price = helper
        .query_config()
        .unwrap()
        .pool_state
        .price_state
        .last_price;
    assert_eq!(
        last_price,
        Decimal256::from_str("1.001187607454013938").unwrap()
    );

    // Check that the shared fees are sent
    let expected_fee_share = 26081u128;
    let recipient_balance = helper.coin_balance(&test_coins[1], &share_recipient);
    assert_eq!(recipient_balance, expected_fee_share);

    let provider = Addr::unchecked("provider");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000_000000u128),
    ];
    helper.give_me_money(&assets, &provider);
    helper.provide_liquidity(&provider, &assets).unwrap();

    let offer_asset = helper.assets[&test_coins[1]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    let last_price = helper
        .query_config()
        .unwrap()
        .pool_state
        .price_state
        .last_price;
    assert_eq!(
        last_price,
        Decimal256::from_str("0.998842355796925899").unwrap()
    );

    helper
        .withdraw_liquidity(&provider, 999_999354, vec![])
        .unwrap();

    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

    let last_price = helper
        .query_config()
        .unwrap()
        .pool_state
        .price_state
        .last_price;
    assert_eq!(
        last_price,
        Decimal256::from_str("1.00118760696709103").unwrap()
    );

    // Disable fee share
    let action = ConcentratedPoolUpdateParams::DisableFeeShare {};
    helper.update_config(&owner, &action).unwrap();

    let config = helper.query_config().unwrap();
    assert!(config.fee_share.is_none());
}

#[test]
fn check_small_trades() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("uluna")];

    let params = ConcentratedPoolParams {
        price_scale: f64_to_dec(4.360000915600192),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params, true).unwrap();

    helper.give_me_money(
        &[
            helper.assets[&test_coins[0]].with_balance(u128::MAX / 2),
            helper.assets[&test_coins[1]].with_balance(u128::MAX / 2),
        ],
        &owner,
    );

    // Fully balanced but small provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(8_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_834862u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    // Trying to mess the last price with lowest possible swap
    for _ in 0..1000 {
        helper.next_block(30);
        let offer_asset = helper.assets[&test_coins[1]].with_balance(1u8);
        helper
            .swap_full_params(&owner, &offer_asset, None, Some(Decimal::MAX))
            .unwrap();
    }

    // Check that after price scale adjustments (even they are small) internal value is still nearly balanced
    let config = helper.query_config().unwrap();
    let pool = helper
        .query_pool()
        .unwrap()
        .assets
        .into_iter()
        .map(|asset| asset.amount.to_decimal256(6u8).unwrap())
        .collect_vec();

    let ixs = [pool[0], pool[1] * config.pool_state.price_state.price_scale];
    let relative_diff = ixs[0].abs_diff(ixs[1]) / max(&ixs).unwrap();

    assert!(
        relative_diff < Decimal256::percent(3),
        "Internal PCL value is off. Relative_diff: {}",
        relative_diff
    );

    // Trying to mess the last price with lowest possible provide
    for _ in 0..1000 {
        helper.next_block(30);
        let assets = vec![helper.assets[&test_coins[1]].with_balance(1u8)];
        helper
            .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.5)))
            .unwrap();
    }

    // Check that after price scale adjustments (even they are small) internal value is still nearly balanced
    let config = helper.query_config().unwrap();
    let pool = helper
        .query_pool()
        .unwrap()
        .assets
        .into_iter()
        .map(|asset| asset.amount.to_decimal256(6u8).unwrap())
        .collect_vec();

    let ixs = [pool[0], pool[1] * config.pool_state.price_state.price_scale];
    let relative_diff = ixs[0].abs_diff(ixs[1]) / max(&ixs).unwrap();

    assert!(
        relative_diff < Decimal256::percent(3),
        "Internal PCL value is off. Relative_diff: {}",
        relative_diff
    );
}

#[test]
fn test_migrate_cl_to_orderbook() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("untrn"), TestCoin::native("astro")];

    let params = ConcentratedPoolParams {
        price_scale: f64_to_dec(0.5),
        ..common_pcl_params()
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params, false).unwrap();

    helper.give_me_money(
        &[
            helper.assets[&test_coins[0]].with_balance(u128::MAX / 2),
            helper.assets[&test_coins[1]].with_balance(u128::MAX / 2),
        ],
        &owner,
    );

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(500_000e6 as u128),
        helper.assets[&test_coins[1]].with_balance(1_000_000e6 as u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    // Make some swaps
    for _ in 0..2 {
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[1]].with_balance(1000e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[0]].with_balance(500e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
    }

    let orders_number = 5;
    let migrate_msg = MigrateMsg::MigrateToOrderbook {
        orderbook_config: OrderbookConfig {
            liquidity_percent: Decimal::percent(20),
            orders_number,
            min_asset_0_order_size: Uint128::from(1000u128),
            min_asset_1_order_size: Uint128::from(1000u128),
            executor: Some(owner.to_string()),
            avg_price_adjustment: f64_to_dec(0.001),
        },
    };

    let new_code_id = helper.app.store_code(pcl_duality_contract());

    // Tweak PCL state to check migration errors
    {
        let mut contract_store = helper.app.contract_storage_mut(&helper.pair_addr);
        set_contract_version(contract_store.as_mut(), "fake_pcl", CONTRACT_VERSION).unwrap()
    };

    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            new_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Can migrate only from astroport-pair-concentrated >=4.0.0, <5.0.0"
    );

    // Checking a major version higher than v4
    {
        let mut contract_store = helper.app.contract_storage_mut(&helper.pair_addr);
        set_contract_version(
            contract_store.as_mut(),
            "astroport-pair-concentrated",
            "15.0.0",
        )
        .unwrap()
    };

    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            new_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Can migrate only from astroport-pair-concentrated >=4.0.0, <5.0.0"
    );

    // Reverting to the correct version
    {
        let mut contract_store = helper.app.contract_storage_mut(&helper.pair_addr);
        set_contract_version(
            contract_store.as_mut(),
            "astroport-pair-concentrated",
            CONTRACT_VERSION,
        )
        .unwrap()
    };

    // for RustRover linter otherwise it assumes that `CONTRACT_NAME` is unused
    _ = CONTRACT_NAME;
    // Try to use general migration path for ordinal PCL contract
    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &MigrateMsg::Migrate {},
            new_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: This endpoint is allowed only for {CONTRACT_NAME}")
    );

    helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            new_code_id,
        )
        .unwrap();

    // Try to use general migration path; Currently is not implemented
    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &MigrateMsg::Migrate {},
            new_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Invalid contract version"
    );

    let config = helper.query_config().unwrap();
    assert_eq!(
        config.pair_info.pair_type,
        PairType::Custom("concentrated_duality_orderbook".to_string())
    );
    assert_eq!(config.pool_state.price_state.price_scale.to_string(), "0.5");
    let ob_state = helper.query_ob_config().unwrap();
    assert!(!ob_state.enabled, "Must be disabled by default");

    // Cant perform PCL transformation again
    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            new_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Can migrate only from astroport-pair-concentrated >=4.0.0, <5.0.0"
    );

    for _ in 0..3 {
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[1]].with_balance(1000e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[0]].with_balance(500e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
    }

    // Confirm that PCL is not posting anything to the orderbook
    // Zero orders since OB integration is disabled
    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        0
    );

    // Enable orderbook
    helper.enable_orderbook(true).unwrap();

    // Still zero
    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        0
    );

    // Perform swaps to trigger orders placement
    for _ in 0..3 {
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[1]].with_balance(1000e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
        helper
            .swap(
                &owner,
                &helper.assets[&test_coins[0]].with_balance(500e6 as u128),
                None,
            )
            .unwrap();
        helper.next_block(1000);
    }

    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        (orders_number * 2) as usize
    );

    // Disable orderbook
    helper.enable_orderbook(false).unwrap();

    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        0
    );

    // Enable again
    helper.enable_orderbook(true).unwrap();

    // Trigger order placement
    helper
        .swap(
            &owner,
            &helper.assets[&test_coins[1]].with_balance(1000e6 as u128),
            None,
        )
        .unwrap();

    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        10
    );

    // Withdraw all liquidity
    let lp_amount = helper.native_balance(&helper.lp_token, &owner);
    helper
        .withdraw_liquidity(&owner, lp_amount, vec![])
        .unwrap();

    assert_eq!(
        helper
            .query_orders(&helper.pair_addr)
            .unwrap()
            .limit_orders
            .len(),
        0
    );
}
