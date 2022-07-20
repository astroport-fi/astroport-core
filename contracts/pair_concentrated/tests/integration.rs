use cosmwasm_std::{Addr, StdError, Uint128};
use itertools::Itertools;
use sim::model::MUL_E18;

use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_concentrated::error::ContractError;
use helper::AppExtension;

use crate::helper::{Helper, TestCoin};

mod helper;

#[test]
fn provide_and_withdraw_no_fee() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: 250,
        out_fee: 250,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let user1 = Addr::unchecked("user1");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.give_me_money(&assets, &user1);

    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(
        100_000_000000u128,
        helper.token_balance(&helper.lp_token, &user1)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));

    // The user2 with the same assets should receive the same share minus NOISE_FEE - 1
    let user2 = Addr::unchecked("user2");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&assets, &user2);
    helper.provide_liquidity(&user2, &assets).unwrap();
    assert_eq!(99_998999, helper.token_balance(&helper.lp_token, &user2));

    // The user3 makes imbalanced provide thus he is charged with SPREAD fees (even there is no usual pool fees)
    let user3 = Addr::unchecked("user3");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(50_000000u128),
        helper.assets[&test_coins[1]].with_balance(150_000000u128),
    ];
    helper.give_me_money(&assets, &user3);
    helper.provide_liquidity(&user3, &assets).unwrap();
    assert_eq!(99_998936, helper.token_balance(&helper.lp_token, &user3));

    // The more provide makes pool imbalanced the more fees are charged
    let user4 = Addr::unchecked("user4");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(0u128),
        helper.assets[&test_coins[1]].with_balance(200_000000u128),
    ];
    helper.give_me_money(&assets, &user4);
    helper.provide_liquidity(&user4, &assets).unwrap();
    assert_eq!(99_998483, helper.token_balance(&helper.lp_token, &user4));

    // Imbalanced provide which makes pool more balanced gives profit to the LP
    let user5 = Addr::unchecked("user5");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(200_000000u128),
        helper.assets[&test_coins[1]].with_balance(0_u128),
    ];
    helper.give_me_money(&assets, &user5);
    helper.provide_liquidity(&user5, &assets).unwrap();
    assert_eq!(99_999508, helper.token_balance(&helper.lp_token, &user5));

    helper
        .withdraw_liquidity(&user1, 100_000000, vec![])
        .unwrap();

    assert_eq!(
        99_900_000000,
        helper.token_balance(&helper.lp_token, &user1)
    );
    // Previous imbalanced provides resulted in slightly different share in assets
    assert_eq!(99_950203, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(100_049804, helper.coin_balance(&test_coins[1], &user1));

    // Checking imbalanced withdraw. Withdrawing only the second asset x 100 with 50 LP tokens
    helper
        .withdraw_liquidity(
            &user2,
            50_000000,
            vec![helper.assets[&test_coins[1]].with_balance(100_000000u128)],
        )
        .unwrap();

    assert_eq!(49_999061, helper.token_balance(&helper.lp_token, &user2));
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(100_000000, helper.coin_balance(&test_coins[1], &user2));

    // Trying to receive more than possible
    let err = helper
        .withdraw_liquidity(
            &user3,
            99_998936,
            vec![helper.assets[&test_coins[1]].with_balance(201_000000u128)],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: Not enough LP tokens. You need 100500253 LP tokens.",
        err.root_cause().to_string()
    );

    // Providing more LP tokens than needed. The rest will be kept on the user's balance
    helper
        .withdraw_liquidity(
            &user3,
            99_998936,
            vec![helper.assets[&test_coins[1]].with_balance(50_000000u128)],
        )
        .unwrap();

    // initial balance - spent amount; the rest goes back to the user3
    assert_eq!(
        99_998936 - 25_000015,
        helper.token_balance(&helper.lp_token, &user3)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user3));
    assert_eq!(50_000000, helper.coin_balance(&test_coins[1], &user3));
}

#[test]
fn provide_with_different_precision() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::cw20precise("FOO", 5),
        TestCoin::cw20precise("BAR", 6),
    ];

    let params = ConcentratedPoolParams {
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: 250,
        out_fee: 250,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
    };

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    for user_name in ["user1", "user2"] {
        let user = Addr::unchecked(user_name);

        let assets = vec![
            helper.assets[&test_coins[0]].with_balance(100_00000u128),
            helper.assets[&test_coins[1]].with_balance(100_000000u128),
        ];
        helper.give_me_money(&assets, &user);

        helper.provide_liquidity(&user, &assets).unwrap();

        assert_eq!(100_000000, helper.token_balance(&helper.lp_token, &user));
        assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
        assert_eq!(0, helper.coin_balance(&test_coins[1], &user));

        helper
            .withdraw_liquidity(&user, 100_000000, vec![])
            .unwrap();

        assert_eq!(0, helper.token_balance(&helper.lp_token, &user));
        assert_eq!(100_00000, helper.coin_balance(&test_coins[0], &user));
        assert_eq!(100_000000, helper.coin_balance(&test_coins[1], &user));
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
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: 250,
        out_fee: 250,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
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
    // // Checking direct swap simulation
    // let sim_resp = helper
    //     .simulate_swap(&offer_asset, Some(helper.assets[&test_coins[2]].clone()))
    //     .unwrap();
    // // And reverse swap as well
    // let reverse_sim_resp = helper
    //     .simulate_reverse_swap(
    //         &helper.assets[&test_coins[2]].with_balance(sim_resp.return_amount.u128()),
    //         Some(helper.assets[&test_coins[0]].clone()),
    //     )
    //     .unwrap();
    // assert_eq!(offer_asset.amount, reverse_sim_resp.offer_amount);

    helper.give_me_money(&[offer_asset.clone()], &user);
    helper.swap(&user, &offer_asset, None).unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
    // 99.999010 x BAR tokens
    // assert_eq!(99_949011, sim_resp.return_amount.u128());
    assert_eq!(99_999496, helper.coin_balance(&test_coins[1], &user));
}

#[test]
fn check_swaps() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: 250,
        out_fee: 250,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
    };
    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let user = Addr::unchecked("user");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);

    // Check swap does not work if pool is empty
    let err = helper
        .swap(
            &user,
            &offer_asset,
            Some(helper.assets[&test_coins[1]].clone()),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: One of the pools is empty"
    );

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    helper
        .swap(
            &user,
            &offer_asset,
            Some(helper.assets[&test_coins[1]].clone()),
        )
        .unwrap();
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user));
    assert_eq!(99_999496, helper.coin_balance(&test_coins[1], &user));

    let offer_asset = helper.assets[&test_coins[0]].with_balance(90_000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user);
    let err = helper
        .swap(
            &user,
            &offer_asset,
            Some(helper.assets[&test_coins[1]].clone()),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::MaxSpreadAssertion {},
        err.downcast().unwrap()
    )
}

#[test]
fn check_wrong_initializations() {
    let owner = Addr::unchecked("owner");

    let mut params = ConcentratedPoolParams {
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: 250,
        out_fee: 250,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
    };

    let err = Helper::new(&owner, vec![TestCoin::native("uluna")], params.clone()).unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: asset_infos must contain exactly two elements",
    );

    params.amp = 0;

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("uluna"), TestCoin::cw20("ASTRO")],
        params.clone(),
    )
    .unwrap_err();

    assert_eq!(
        ContractError::IncorrectPoolParam("amp".to_string(), 1000, 1000000000),
        err.downcast().unwrap(),
    );
}

/*
#[test]
fn check_withdraw_charges_fees() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native("uluna"),
        TestCoin::cw20("USDC"),
        TestCoin::cw20("USDD"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), 100u64, None).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000_000000u128),
        helper.assets[&test_coins[2]].with_balance(100_000_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let user1 = Addr::unchecked("user1");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(100_000000u128);

    // Usual swap for reference
    helper.give_me_money(&[offer_asset.clone()], &user1);
    helper
        .swap(
            &user1,
            &offer_asset,
            Some(helper.assets[&test_coins[1]].clone()),
        )
        .unwrap();
    let usual_swap_amount = helper.coin_balance(&test_coins[1], &user1);
    assert_eq!(99_950000, usual_swap_amount);

    // Trying to swap LUNA -> USDC via provide/withdraw
    let user2 = Addr::unchecked("user2");
    helper.give_me_money(&[offer_asset.clone()], &user2);

    // Provide 100 x LUNA
    helper.provide_liquidity(&user2, &[offer_asset]).unwrap();

    // Withdraw 100 x USDC
    let lp_tokens_amount = helper.token_balance(&helper.lp_token, &user2);
    let err = helper
        .withdraw_liquidity(
            &user2,
            lp_tokens_amount,
            vec![helper.assets[&test_coins[1]].with_balance(100_000000u128)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Not enough LP tokens. You need 100025000 LP tokens."
    );

    helper
        .withdraw_liquidity(
            &user2,
            lp_tokens_amount,
            vec![helper.assets[&test_coins[1]].with_balance(usual_swap_amount)],
        )
        .unwrap();

    // A small residual of LP tokens is left
    assert_eq!(16, helper.token_balance(&helper.lp_token, &user2));
    assert_eq!(
        usual_swap_amount,
        helper.coin_balance(&test_coins[1], &user2)
    );
}

#[test]
fn check_5pool_prices() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native("uusd"),
        TestCoin::cw20("USDX"),
        TestCoin::cw20("USDY"),
        TestCoin::cw20("USDZ"),
        TestCoin::native("ibc/usd"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), 100u64, None).unwrap();

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
        helper.assets[&test_coins[2]].with_balance(100_000_000_000000u128),
        helper.assets[&test_coins[3]].with_balance(100_000_000_000000u128),
        helper.assets[&test_coins[4]].with_balance(100_000_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();
    check_prices(&helper);

    helper.app.next_block(1000);

    let user1 = Addr::unchecked("user1");
    let offer_asset = helper.assets[&test_coins[0]].with_balance(1000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);

    helper
        .swap(
            &user1,
            &offer_asset,
            Some(helper.assets[&test_coins[1]].clone()),
        )
        .unwrap();
    check_prices(&helper);

    helper.app.next_block(86400);

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
        helper.assets[&test_coins[2]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&assets, &user1);

    // Imbalanced provide
    helper.provide_liquidity(&user1, &assets).unwrap();
    check_prices(&helper);

    helper.app.next_block(14 * 86400);

    let offer_asset = helper.assets[&test_coins[3]].with_balance(10_000_000000u128);
    helper.give_me_money(&[offer_asset.clone()], &user1);
    helper
        .swap(
            &user1,
            &offer_asset,
            Some(helper.assets[&test_coins[4]].clone()),
        )
        .unwrap();
    check_prices(&helper);
}*/
