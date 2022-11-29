use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_multi_test::next_block;
use itertools::Itertools;

use astroport::asset::{native_asset_info, AssetInfoExt, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, PromoteParams, UpdatePoolParams,
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
        initial_price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        owner: None,
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
    wrong_params.initial_price_scale = Decimal::zero();

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
        initial_price_scale: Decimal::from_ratio(2u8, 1u8),
        ma_half_time: 600,
        owner: None,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    // checking LP token price on an empty pool
    let lp_price = helper.query_lp_price().unwrap();
    assert_eq!(lp_price, 0.0);

    let user1 = Addr::unchecked("user1");

    // Try to provide with additional wrong asset
    let wrong_assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
        native_asset_info("random_coin".to_string()).with_balance(100u8),
    ];
    helper.give_me_money(&wrong_assets, &user1);
    let err = helper.provide_liquidity(&user1, &wrong_assets).unwrap_err();
    assert_eq!(
        ContractError::InvalidNumberOfAssets(2usize),
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

    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(70710_677118, helper.token_balance(&helper.lp_token, &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    // Check virtual LP token price grows
    // let lp_price = helper.query_lp_price().unwrap();
    // assert_eq!(lp_price, ?);

    // The user2 with the same assets should receive the same share
    // (except MINIMUM_LIQUIDITY_AMOUNT bc of 1st provide)
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

    // LP token price grows up because of noise fees
    // let lp_price = helper.query_lp_price().unwrap();
    // assert_eq!(lp_price, ?);

    // user1 withdraws one 10th
    helper
        .withdraw_liquidity(&user1, 7071_067711, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 - 7071_067711,
        helper.token_balance(&helper.lp_token, &user1)
    );
    // initial provide charged small fee thus user1 received slightly less fees
    assert_eq!(9999_999857, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(4999_999928, helper.coin_balance(&test_coins[1], &user1));

    // user2 withdraws half
    helper
        .withdraw_liquidity(&user2, 35355_339059, vec![])
        .unwrap();

    assert_eq!(
        70710_677118 + MINIMUM_LIQUIDITY_AMOUNT.u128() - 35355_339059,
        helper.token_balance(&helper.lp_token, &user2)
    );
    assert_eq!(50000000000, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(25000000000, helper.coin_balance(&test_coins[1], &user2));
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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
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
    assert_eq!(
        offer_asset.amount,
        reverse_sim_resp.offer_amount + Uint128::one() // slight error appears due to dynamic fee rate
    );

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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
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
    let wrong_coin = native_asset_info("random_coin".to_string());
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
    assert_eq!(99_741244, helper.coin_balance(&test_coins[0], &user2));

    let d = helper.query_d().unwrap();
    assert_eq!(dec_to_f64(d), 200000.520827);
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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
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

    for i in 0..3 {
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
    for i in 0..3 {
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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

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
        initial_price_scale: Decimal::one(),
        ma_half_time: 600,
        owner: None,
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
                        from.eq(&helper.assets[&from_coin]) && to.eq(&helper.assets[&to_coin])
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
