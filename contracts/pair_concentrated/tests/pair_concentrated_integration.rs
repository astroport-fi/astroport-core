#![cfg(not(tarpaulin_include))]

use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;

use cosmwasm_std::{Addr, Coin, Decimal, Decimal256, StdError, Uint128};
use itertools::{max, Itertools};

use astroport::asset::{
    native_asset_info, Asset, AssetInfo, AssetInfoExt, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::cosmwasm_ext::{AbsDiff, IntegerToDecimal};
use astroport::observation::OracleObservation;
use astroport::pair::{ExecuteMsg, PoolResponse, MAX_FEE_SHARE_BPS};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, PromoteParams, QueryMsg, UpdatePoolParams,
};
use astroport_mocks::cw_multi_test::{BasicApp, Executor};
use astroport_mocks::{astroport_address, MockConcentratedPairBuilder, MockGeneratorBuilder};
use astroport_pair_concentrated::error::ContractError;
use astroport_pcl_common::consts::{AMP_MAX, AMP_MIN, MA_HALF_TIME_LIMITS};
use astroport_pcl_common::error::PclError;

use crate::helper::{common_pcl_params, dec_to_f64, f64_to_dec, AppExtension, Helper, TestCoin};

mod helper;

#[test]
fn check_observe_queries() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000f64);

    assert_eq!(0, helper.coin_balance(&test_coins[1], &user));
    helper.swap(&user, &offer_asset, None).unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
    assert_eq!(99_737929, helper.coin_balance(&test_coins[1], &user));

    helper.app.next_block(1000);

    let user2 = Addr::unchecked("user2");
    let offer_asset = helper.assets[&test_coins[1]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user2);
    helper.swap(&user2, &offer_asset, None).unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user2));
    assert_eq!(99_741246, helper.coin_balance(&test_coins[0], &user2));

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000.260415);

    let res: OracleObservation = helper
        .app
        .wrap()
        .query_wasm_smart(
            helper.pair_addr.to_string(),
            &QueryMsg::Observe { seconds_ago: 0 },
        )
        .unwrap();

    assert_eq!(
        res,
        OracleObservation {
            timestamp: helper.app.block_info().time.seconds(),
            price: Decimal::from_str("1.002627596167552265").unwrap()
        }
    );
}

#[test]
fn check_wrong_initialization() {
    let owner = Addr::unchecked("owner");

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let err = Helper::new(&owner, vec![TestCoin::native("uluna")], params.clone()).unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: asset_infos must contain exactly two elements",
    );

    let mut wrong_params = params.clone();
    wrong_params.amp = Decimal::zero();

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("uluna"), TestCoin::cw20("ASTRO")],
        wrong_params,
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
        vec![TestCoin::native("uluna"), TestCoin::cw20("ASTRO")],
        wrong_params,
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
        vec![TestCoin::native("uluna"), TestCoin::cw20("ASTRO")],
        wrong_params,
    )
    .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Initial price scale can not be zero",
    );

    // check instantiation with valid params
    Helper::new(
        &owner,
        vec![TestCoin::native("uluna"), TestCoin::cw20("ASTRO")],
        params,
    )
    .unwrap();
}

#[test]
fn check_create_pair_with_unsupported_denom() {
    let owner = Addr::unchecked("owner");

    let wrong_coins = vec![TestCoin::native("rc"), TestCoin::cw20("USDC")];
    let valid_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let err = Helper::new(&owner, wrong_coins.clone(), params.clone()).unwrap_err();
    assert_eq!(
        "Generic error: Invalid denom length [3,128]: rc",
        err.root_cause().to_string()
    );

    Helper::new(&owner, valid_coins.clone(), params.clone()).unwrap();
}

#[test]
fn provide_and_withdraw() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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
    let err = helper.provide_liquidity(&user1, &wrong_assets).unwrap_err();
    assert_eq!(
        "Generic error: Unexpected asset random-coin",
        err.root_cause().to_string()
    );

    // Provide with asset which does not belong to the pair
    let err = helper
        .provide_liquidity(
            &user1,
            &[
                random_coin.clone(),
                helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
            ],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: Unexpected asset random-coin",
        err.root_cause().to_string()
    );

    let err = helper
        .provide_liquidity(&user1, &[random_coin.clone()])
        .unwrap_err();
    assert_eq!(
        "The asset random-coin does not belong to the pair",
        err.root_cause().to_string()
    );

    let err = helper.provide_liquidity(&user1, &[]).unwrap_err();
    assert_eq!(
        "Generic error: Nothing to provide",
        err.root_cause().to_string()
    );

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
    helper.give_me_money(
        &[helper.assets[&test_coins[1]].with_balance(50_000_000000u128)],
        &user1,
    );

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

    assert_eq!(70710_677118, helper.token_balance(&helper.lp_token, &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    assert_eq!(
        helper
            .query_share(helper.token_balance(&helper.lp_token, &user1))
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
        helper.token_balance(&helper.lp_token, &user2)
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
        helper.token_balance(&helper.lp_token, &user3)
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
    assert_eq!(62217_722016, helper.token_balance(&helper.lp_token, &user4));

    // One of assets may be omitted
    let user5 = Addr::unchecked("user5");
    let assets = vec![helper.assets[&test_coins[0]].with_balance(140_000_000000u128)];
    helper.give_me_money(&assets, &user5);
    helper.provide_liquidity(&user5, &assets).unwrap();
    assert_eq!(57271_023590, helper.token_balance(&helper.lp_token, &user5));

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
        helper.token_balance(&helper.lp_token, &user1)
    );
    assert_eq!(9382_010960, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(5330_688045, helper.coin_balance(&test_coins[1], &user1));

    // user2 withdraws half
    helper
        .withdraw_liquidity(&user2, 35355_339059, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128() - 35355_339059,
        helper.token_balance(&helper.lp_token, &user2)
    );
    assert_eq!(46910_055478, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(26653_440612, helper.coin_balance(&test_coins[1], &user2));
}

#[test]
fn check_imbalanced_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params.clone()).unwrap();

    let user1 = Addr::unchecked("user1");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    // Making two provides just to check that both if-branches are covered (initial and usual provide)
    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        200495_366531,
        helper.token_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    // creating a new pool with inverted price scale
    params.price_scale = Decimal::from_ratio(1u8, 2u8);

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        200495_366531,
        helper.token_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));
}

#[test]
fn provide_with_different_precision() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::cw20precise("FOO", 5),
        TestCoin::cw20precise("BAR", 6),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_00000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];

    helper.provide_liquidity(&owner, &assets).unwrap();

    let tolerance = 9;

    for user_name in ["user1", "user2", "user3"] {
        let user = Addr::unchecked(user_name);

        helper.give_me_money(&assets, &user);

        helper.provide_liquidity(&user, &assets).unwrap();

        let lp_amount = helper.token_balance(&helper.lp_token, &user);
        assert!(
            100_000000 - lp_amount < tolerance,
            "LP token balance assert failed for {user}"
        );
        assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
        assert_eq!(0, helper.coin_balance(&test_coins[1], &user));

        helper.withdraw_liquidity(&user, lp_amount, vec![]).unwrap();

        assert_eq!(0, helper.token_balance(&helper.lp_token, &user));
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
        TestCoin::cw20precise("FOO", 5),
        TestCoin::cw20precise("BAR", 6),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_00000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
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
fn check_reverse_swap() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::cw20("FOO"), TestCoin::cw20("BAR")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
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

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

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
    helper.provide_liquidity(&owner, &assets).unwrap();

    // trying to swap cw20 without calling Cw20::Send method
    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &ExecuteMsg::Swap {
                offer_asset: helper.assets[&test_coins[1]].with_balance(1u8),
                ask_asset_info: None,
                belief_price: None,
                max_spread: None,
                to: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(ContractError::Cw20DirectSwap {}, err.downcast().unwrap());

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000f64);

    assert_eq!(0, helper.coin_balance(&test_coins[1], &user));
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

    let price1 = helper.observe_price(0).unwrap();
    helper.app.next_block(10);
    // Swapping the lowest amount possible which results in positive return amount
    helper
        .swap(
            &user,
            &helper.assets[&test_coins[1]].with_balance(2u128),
            None,
        )
        .unwrap();
    let price2 = helper.observe_price(0).unwrap();
    // With such a small swap size contract doesn't store observation
    assert_eq!(price1, price2);

    helper.app.next_block(10);
    // Swap the smallest possible amount which gets observation saved
    helper
        .swap(
            &user,
            &helper.assets[&test_coins[1]].with_balance(1005u128),
            None,
        )
        .unwrap();
    let price3 = helper.observe_price(0).unwrap();
    // Prove that price didn't jump that much
    let diff = price3.diff(price2);
    assert!(
        diff / price2 < f64_to_dec(0.005),
        "price jumped from {price2} to {price3} which is more than 0.5%"
    );
}

#[test]
fn check_swaps_with_price_update() {
    let owner = Addr::unchecked("owner");
    let half = Decimal::from_ratio(1u8, 2u8);

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    helper.app.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.app.next_block(1000);

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
        helper.app.next_block(1000);
    }

    let offer_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000u128);
    for _i in 0..4 {
        helper.give_me_money(&[offer_asset.clone()], &user1);
        helper.swap(&user1, &offer_asset, Some(half)).unwrap();
        helper.app.next_block(1000);
    }
}

#[test]
fn provides_and_swaps() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    helper.app.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.app.next_block(1000);

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

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.0001),
        ..common_pcl_params()
    };
    let mut helper = Helper::new(&owner, test_coins, params).unwrap();

    let random_user = Addr::unchecked("random");
    let action = ConcentratedPoolUpdateParams::Update(UpdatePoolParams {
        mid_fee: Some(f64_to_dec(0.002)),
        out_fee: None,
        fee_gamma: None,
        repeg_profit_threshold: None,
        min_price_scale_delta: None,
        ma_half_time: None,
    });

    let err = helper.update_config(&random_user, &action).unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    helper.update_config(&owner, &action).unwrap();

    helper.app.next_block(86400);

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

    helper.app.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 42f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.000095);
    assert_eq!(amp_gamma.future_time, future_time);

    helper.app.next_block(50_000);

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

    helper.app.next_block(50_000);

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

    helper.app.next_block(50_000);

    let amp_gamma = helper.query_amp_gamma().unwrap();
    assert_eq!(dec_to_f64(amp_gamma.amp), 42f64);
    assert_eq!(dec_to_f64(amp_gamma.gamma), 0.0000945);
    assert_eq!(amp_gamma.future_time, last_change_time);
}

#[test]
fn check_prices() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("USDX")];

    let helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();
    let err = helper.query_prices().unwrap_err();
    assert_eq!(StdError::generic_err("Querier contract error: Generic error: Not implemented.Use { \"observe\" : { \"seconds_ago\" : ... } } instead.")
    , err);
}

#[test]
fn update_owner() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("USDX")];

    let mut helper = Helper::new(&owner, test_coins, common_pcl_params()).unwrap();

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
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("USDX")];

    // create pair with test_coins
    let helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    // query current pool D value before providing any liquidity
    let err = helper.query_d().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Generic error: Pools are empty"
    );
}

#[test]
fn asset_balances_tracking_without_in_params() {
    let owner = Addr::unchecked("owner");
    let user1 = Addr::unchecked("user1");
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::native("uusd")];

    // Instantiate pair without asset balances tracking
    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(5_000000u128),
        helper.assets[&test_coins[1]].with_balance(5_000000u128),
    ];

    // Check that asset balances are not tracked
    // The query AssetBalanceAt returns None for this case
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    // Enable asset balances tracking
    helper
        .update_config(
            &owner,
            &ConcentratedPoolUpdateParams::EnableAssetBalancesTracking {},
        )
        .unwrap();

    // Check that asset balances were not tracked before this was enabled
    // The query AssetBalanceAt returns None for this case
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    // Check that asset balances had zero balances before next block upon tracking enabling
    helper.app.update_block(|b| b.height += 1);

    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.unwrap().is_zero());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.unwrap().is_zero());

    helper.give_me_money(&assets, &user1);
    helper.provide_liquidity(&user1, &assets).unwrap();

    // Check that asset balances changed after providing liqudity
    helper.app.update_block(|b| b.height += 1);
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(5_000000));

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(5_000000));
}

#[test]
fn asset_balances_tracking_with_in_params() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::native("uusd")];

    let params = ConcentratedPoolParams {
        track_asset_balances: Some(true),
        ..common_pcl_params()
    };

    // Instantiate pair without asset balances tracking
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(5_000000u128),
        helper.assets[&test_coins[1]].with_balance(5_000000u128),
    ];

    // Check that enabling asset balances tracking can not be done if it is already enabled
    let err = helper
        .update_config(
            &owner,
            &ConcentratedPoolUpdateParams::EnableAssetBalancesTracking {},
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AssetBalancesTrackingIsAlreadyEnabled {}
    );
    // Check that asset balances were not tracked before instantiation
    // The query AssetBalanceAt returns None for this case
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    // Check that asset balances were not tracked before instantiation
    // The query AssetBalanceAt returns None for this case
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.is_none());

    // Check that asset balances had zero balances before next block upon instantiation
    helper.app.update_block(|b| b.height += 1);

    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.unwrap().is_zero());

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert!(res.unwrap().is_zero());

    // Provide liquidity
    helper
        .provide_liquidity(
            &owner,
            &[
                assets[0].info.with_balance(999_000000u128),
                assets[1].info.with_balance(1000_000000u128),
            ],
        )
        .unwrap();

    assert_eq!(
        helper.token_balance(&helper.lp_token, &owner),
        999_498998u128
    );

    // Check that asset balances changed after providing liquidity
    helper.app.update_block(|b| b.height += 1);
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(999_000000));

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(1000_000000));

    // Swap
    helper
        .swap(
            &owner,
            &Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                amount: Uint128::new(1_000000),
            },
            None,
        )
        .unwrap();

    // Check that asset balances changed after swapping
    helper.app.update_block(|b| b.height += 1);
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(998_001335));

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(1001_000000));

    // Withdraw liquidity
    helper
        .withdraw_liquidity(&owner, 500_000000, vec![])
        .unwrap();

    // Check that asset balances changed after withdrawing
    helper.app.update_block(|b| b.height += 1);
    let res = helper
        .query_asset_balance_at(&assets[0].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(498_751043));

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(500_249625));
}

#[test]
fn provides_and_swaps_and_withdraw() {
    let owner = Addr::unchecked("owner");
    let half = Decimal::from_ratio(1u8, 2u8);
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        price_scale: Decimal::from_ratio(1u8, 2u8),
        ..common_pcl_params()
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    helper.app.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(200_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    // swap uluna
    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(1000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, Some(half)).unwrap();

    helper.app.next_block(1000);

    // swap usdc
    let offer_asset = helper.assets[&test_coins[1]].with_balance(1000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, Some(half)).unwrap();

    let offer_asset = helper.assets[&test_coins[1]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, Some(half)).unwrap();

    // swap uluna
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, Some(half)).unwrap();
    let res: PoolResponse = helper
        .app
        .wrap()
        .query_wasm_smart(helper.pair_addr.to_string(), &QueryMsg::Pool {})
        .unwrap();

    assert_eq!(res.total_share.u128(), 141_421_356_237u128);
    let owner_balance = helper.token_balance(&helper.lp_token, &owner);

    helper
        .withdraw_liquidity(&owner, owner_balance, vec![])
        .unwrap();
    let res: PoolResponse = helper
        .app
        .wrap()
        .query_wasm_smart(helper.pair_addr.to_string(), &QueryMsg::Pool {})
        .unwrap();

    assert_eq!(res.total_share.u128(), 1000u128);
}

#[test]
fn provide_liquidity_with_autostaking_to_generator() {
    let astroport = astroport_address();

    let app = Rc::new(RefCell::new(BasicApp::new(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &astroport,
                vec![Coin {
                    denom: "ustake".to_owned(),
                    amount: Uint128::new(1_000_000_000000),
                }],
            )
            .unwrap();
    })));

    let generator = MockGeneratorBuilder::new(&app).instantiate();

    let factory = generator.factory();

    let astro_token_info = generator.astro_token_info();
    let ustake = native_asset_info("ustake".to_owned());

    let pair = MockConcentratedPairBuilder::new(&app)
        .with_factory(&factory)
        .with_asset(&astro_token_info)
        .with_asset(&ustake)
        .instantiate(None);

    pair.mint_allow_provide_and_stake(
        &astroport,
        &[
            astro_token_info.with_balance(1_000_000000u128),
            ustake.with_balance(1_000_000000u128),
        ],
    );

    assert_eq!(pair.lp_token().balance(&pair.address), Uint128::new(1000));
    assert_eq!(
        generator.query_deposit(&pair.lp_token(), &astroport),
        Uint128::new(999_999000),
    );
}

#[test]
fn provide_withdraw_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("uluna")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(10u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_938039u128),
        helper.assets[&test_coins[1]].with_balance(1_093804u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();
    helper.app.next_block(90);
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.app.next_block(90);
    let uusd = helper.assets[&test_coins[0]].with_balance(5_000000u128);
    helper.swap(&owner, &uusd, Some(f64_to_dec(0.5))).unwrap();

    helper.app.next_block(600);
    // Withdraw all
    let lp_amount = helper.token_balance(&helper.lp_token, &owner);
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

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("uluna")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(10u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // Fully balanced provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000000u128),
    ];
    helper
        .provide_liquidity_with_slip_tolerance(&owner, &assets, Some(f64_to_dec(0.02)))
        .unwrap();

    // Imbalanced provide. Slippage is more than 2% while we enforce 2% max slippage
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(5_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000000u128),
    ];
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
}

#[test]
fn test_frontrun_before_initial_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::native("uluna")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        price_scale: Decimal::from_ratio(10u8, 1u8),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // Random person tries to frontrun initial provide and imbalance pool upfront
    helper
        .app
        .send_tokens(
            owner.clone(),
            helper.pair_addr.clone(),
            &[helper.assets[&test_coins[0]]
                .with_balance(10_000_000000u128)
                .as_coin()
                .unwrap()],
        )
        .unwrap();

    // Fully balanced provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(10_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();
    // Now pool became imbalanced with value (10010, 1)  (or in internal representation (10010, 10))
    // while price scale stays 10

    let arber = Addr::unchecked("arber");
    let offer_asset_luna = helper.assets[&test_coins[1]].with_balance(1_000000u128);
    // Arber spinning pool back to balanced state
    loop {
        helper.app.next_block(10);
        helper.give_me_money(&[offer_asset_luna.clone()], &arber);
        // swapping until price satisfies an arber
        if helper
            .swap_full_params(
                &arber,
                &offer_asset_luna,
                Some(f64_to_dec(0.02)),
                Some(f64_to_dec(0.1)), // imagine market price is 10 -> i.e. inverted price is 1/10
            )
            .is_err()
        {
            break;
        }
    }

    // price scale changed, however it isn't equal to 10 because of repegging
    // But next swaps will align price back to the market value
    let config = helper.query_config().unwrap();
    let price_scale = config.pool_state.price_state.price_scale;
    assert!(
        dec_to_f64(price_scale) - 77.255853 < 1e-5,
        "price_scale: {price_scale} is far from expected price",
    );

    // Arber collected significant profit (denominated in uusd)
    // Essentially 10_000 - fees (which settled in the pool)
    let arber_balance = helper.coin_balance(&test_coins[0], &arber);
    assert_eq!(arber_balance, 9667_528248);

    // Pool's TVL increased from (10, 1) i.e. 20 to (320, 32) i.e. 640 considering market price is 10.0
    let pools = config
        .pair_info
        .query_pools(&helper.app.wrap(), &helper.pair_addr)
        .unwrap();
    assert_eq!(pools[0].amount.u128(), 320_624088);
    assert_eq!(pools[1].amount.u128(), 32_000000);
}

#[test]
fn check_correct_fee_share() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut helper = Helper::new(&owner, test_coins.clone(), common_pcl_params()).unwrap();

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

    // Attempt setting fee share with max+1 fee share
    let action = ConcentratedPoolUpdateParams::EnableFeeShare {
        fee_share_bps: 0,
        fee_share_address: share_recipient.to_string(),
    };
    let err = helper.update_config(&owner, &action).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::FeeShareOutOfBounds {}
    );

    helper.app.next_block(1000);

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

    helper.app.next_block(1000);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper.app.next_block(1000);

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

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

    helper
        .withdraw_liquidity(&provider, 999_999354, vec![])
        .unwrap();

    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();

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

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // Fully balanced but small provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(8_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_834862u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    // Trying to mess the last price with lowest possible swap
    for _ in 0..1000 {
        helper.app.next_block(30);
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
        helper.app.next_block(30);
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
fn check_small_trades_18decimals() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::cw20precise("ETH", 18),
        TestCoin::cw20precise("USD", 18),
    ];

    let params = ConcentratedPoolParams {
        price_scale: f64_to_dec(4.360000915600192),
        ..common_pcl_params()
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // Fully balanced but small provide
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(8e18 as u128),
        helper.assets[&test_coins[1]].with_balance(1_834862000000000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    // Trying to mess the last price with lowest possible swap
    for _ in 0..1000 {
        helper.app.next_block(30);
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
        helper.app.next_block(30);
        // 0.000001 USD. minimum provide is limited to LP token precision which is 6 decimals.
        let assets = vec![helper.assets[&test_coins[1]].with_balance(1000000000000u128)];
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
