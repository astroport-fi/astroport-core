use crate::helper::{f64_to_dec, AppExtension, Helper, TestCoin};
use astroport::asset::{AssetInfoExt, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_concentrated::error::ContractError;
use cosmwasm_std::{Addr, Decimal};

mod helper;

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
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];
    helper.give_me_money(&assets, &user1);

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

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

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

    for _ in 0..3 {
        helper.give_me_money(&[offer_asset.clone()], &user1);
        helper.swap(&user1, &offer_asset, Some(half)).unwrap();
        let new_vlp_price = helper.query_lp_price().unwrap();
        assert!(
            new_vlp_price > prev_vlp_price,
            "new_vlp_price < prev_vlp_price"
        );
        prev_vlp_price = new_vlp_price;
        helper.app.next_block(1000);
    }

    let offer_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000u128);
    for _ in 0..3 {
        helper.give_me_money(&[offer_asset.clone()], &user1);
        helper.swap(&user1, &offer_asset, Some(half)).unwrap();
        let new_vlp_price = helper.query_lp_price().unwrap();
        assert!(
            new_vlp_price > prev_vlp_price,
            "new_vlp_price < prev_vlp_price"
        );
        prev_vlp_price = new_vlp_price;
        helper.app.next_block(1000);
    }
}
