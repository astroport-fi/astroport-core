use cosmwasm_std::{Addr, Decimal, Uint128};

use cw_multi_test::{next_block, Executor};
use itertools::Itertools;

use astroport::asset::{
    native_asset_info, Asset, AssetInfo, AssetInfoExt, MINIMUM_LIQUIDITY_AMOUNT,
};

use astroport::pair::{ExecuteMsg, PoolResponse};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, PromoteParams, QueryMsg, UpdatePoolParams,
};
use astroport_pair_concentrated::consts::{AMP_MAX, AMP_MIN, MA_HALF_TIME_LIMITS};
use astroport_pair_concentrated::error::ContractError;

use crate::helper::{dec_to_f64, f64_to_dec, AppExtension, Helper, TestCoin};

mod helper;

#[test]
fn check_wrong_initialization() {
    let owner = Addr::unchecked("owner");

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        track_asset_balances: None,
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
        ContractError::IncorrectPoolParam(
            "amp".to_string(),
            AMP_MIN.to_string(),
            AMP_MAX.to_string()
        ),
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
        ContractError::IncorrectPoolParam(
            "ma_half_time".to_string(),
            MA_HALF_TIME_LIMITS.start().to_string(),
            MA_HALF_TIME_LIMITS.end().to_string()
        ),
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

    let wrong_coins = vec![TestCoin::native("random_coin"), TestCoin::cw20("USDC")];
    let valid_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let err = Helper::new(&owner, wrong_coins.clone(), params.clone()).unwrap_err();
    assert_eq!(
        "Generic error: Native denom is not in expected format [a-zA-Z\\-][3,60]: random_coin",
        err.root_cause().to_string()
    );

    Helper::new(&owner, valid_coins.clone(), params.clone()).unwrap();
}

#[test]
fn provide_and_withdraw() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // checking LP token virtual price on an empty pool
    let lp_price = helper.query_lp_price().unwrap();
    assert_eq!(lp_price, 0.0);

    let user1 = Addr::unchecked("user1");

    let random_coin = native_asset_info("random-coin".to_string()).with_balance(100u8);
    let wrong_assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        random_coin.clone(),
    ];
    helper.give_me_money(&wrong_assets, &user1);
    let err = helper.provide_liquidity(&user1, &wrong_assets).unwrap_err();
    assert_eq!(
        "Generic error: Asset random-coin is not in the pool",
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
        "Generic error: Asset random-coin is not in the pool",
        err.root_cause().to_string()
    );

    let err = helper
        .provide_liquidity(&user1, &[random_coin])
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
    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(70710_677118, helper.token_balance(&helper.lp_token, &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

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
    assert_eq!(9382_010962, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(5330_688046, helper.coin_balance(&test_coins[1], &user1));

    // user2 withdraws half
    helper
        .withdraw_liquidity(&user2, 35355_339059, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128() - 35355_339059,
        helper.token_balance(&helper.lp_token, &user2)
    );
    assert_eq!(46910_055479, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(26653_440613, helper.coin_balance(&test_coins[1], &user2));
}

#[test]
fn check_imbalanced_provide() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let mut params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        track_asset_balances: None,
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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_00000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];

    helper.provide_liquidity(&owner, &assets).unwrap();

    for user_name in ["user1", "user2", "user3"] {
        let user = Addr::unchecked(user_name);

        helper.give_me_money(&assets, &user);

        helper.provide_liquidity(&user, &assets).unwrap();

        assert_eq!(100_000000, helper.token_balance(&helper.lp_token, &user));
        assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
        assert_eq!(0, helper.coin_balance(&test_coins[1], &user));

        helper
            .withdraw_liquidity(&user, 100_000000, vec![])
            .unwrap();

        assert_eq!(0, helper.token_balance(&helper.lp_token, &user));
        assert_eq!(
            100_00000,
            helper.coin_balance(&test_coins[0], &user),
            "Withdrawn amount of coin0 assert failed for {user}"
        );
        assert_eq!(
            100_000000,
            helper.coin_balance(&test_coins[1], &user),
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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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
        ContractError::MaxSpreadAssertion {},
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

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();
    helper.app.update_block(next_block);

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
    helper.provide_liquidity(&owner, &assets).unwrap();
    check_prices(&helper);

    helper.app.next_block(1000);

    let user1 = Addr::unchecked("user1");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(1000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);

    helper.swap(&user1, &offer_asset, None).unwrap();
    check_prices(&helper);

    helper.app.next_block(86400);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&assets, &user1);

    helper.provide_liquidity(&user1, &assets).unwrap();
    check_prices(&helper);

    helper.app.next_block(14 * 86400);

    let offer_asset = helper.assets[&test_coins[1]].with_balance(10_000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);
    helper.swap(&user1, &offer_asset, None).unwrap();
    check_prices(&helper);
}

#[test]
fn update_owner() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("USDX")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    let mut helper = Helper::new(&owner, test_coins, params).unwrap();

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
    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };
    // create pair with test_coins
    let helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
    };

    // Instantiate pair without asset balances tracking
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: Some(true),
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
    assert_eq!(res.unwrap(), Uint128::new(498_751042));

    let res = helper
        .query_asset_balance_at(&assets[1].info, helper.app.block_info().height)
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(500_249624));
}

#[test]
fn provides_and_swaps_and_withdraw() {
    let owner = Addr::unchecked("owner");
    let half = Decimal::from_ratio(1u8, 2u8);
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::from_ratio(1u8, 2u8),
        ma_half_time: 600,
        track_asset_balances: None,
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
