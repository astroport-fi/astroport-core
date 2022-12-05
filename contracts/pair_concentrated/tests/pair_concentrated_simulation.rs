extern crate core;

mod helper;

use crate::helper::{f64_to_dec, AppExtension, Helper, TestCoin};
use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_concentrated::error::ContractError;
use cosmwasm_std::{Addr, Decimal};
use proptest::prelude::*;
use std::collections::HashMap;

const MAX_EVENTS: usize = 100;

fn simulate_case(case: Vec<(usize, u128, u64)>) {
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");

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

    let balances = vec![100_000_000_000000u128, 100_000_000_000000u128];

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(balances[0]),
        helper.assets[&test_coins[1]].with_balance(balances[1]),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let mut i = 0;
    for (offer_ind, dy, shift_time) in case {
        let _ask_ind = 1 - offer_ind;

        println!("i: {i}, {offer_ind} {dy} {shift_time}");
        let offer_asset = helper.assets[&test_coins[offer_ind]].with_balance(dy);
        // let balance_before = helper.coin_balance(&test_coins[ask_ind], &user);
        helper.give_me_money(&[offer_asset.clone()], &user);
        if let Err(err) = helper.swap(&user, &offer_asset, None) {
            let err: ContractError = err.downcast().unwrap();
            match err {
                ContractError::MaxSpreadAssertion {} => {
                    // if swap fails because of spread then skip this case
                    println!("exceeds spread limit");
                }
                _ => panic!("{err}"),
            }

            i += 1;
            continue;
        };
        // let swap_amount = helper.coin_balance(&test_coins[ask_ind], &user) - balance_before;

        i += 1;

        // Shift time so EMA will update oracle prices
        helper.app.next_block(shift_time);
    }
}

fn simulate_provide_case(case: Vec<(impl Into<String>, u128, u128)>) {
    let owner = Addr::unchecked("owner");
    let tolerance = 1e-6; // allowed loss per provide due to integer math withing contract

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20precise("USDC", 10)];

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

    // owner makes the first provide cuz the pool charges small amount
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000_0000000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let mut accounts: HashMap<Addr, (u128, u128, u8)> = HashMap::new();
    for (user, coin0_amnt, coin1_amnt) in case {
        let user = Addr::unchecked(user);
        println!("{user} {coin0_amnt} {coin1_amnt}");
        let assets = vec![
            helper.assets[&test_coins[0]].with_balance(coin0_amnt),
            helper.assets[&test_coins[1]].with_balance(coin1_amnt),
        ];
        helper.give_me_money(&assets, &user);
        helper.provide_liquidity(&user, &assets).unwrap();

        let entry = accounts.entry(user).or_default();
        (*entry).0 = entry.0 + coin0_amnt;
        (*entry).1 = entry.1 + coin1_amnt;
        (*entry).2 += 1;
    }

    for (user, &(coin0_amnt, coin1_amnt, cnt)) in &accounts {
        println!("Checking user {user}");

        let lp_amount = helper.token_balance(&helper.lp_token, user);
        helper.withdraw_liquidity(user, lp_amount, vec![]).unwrap();

        let coin0_amnt = coin0_amnt as f64;
        let coin1_amnt = coin1_amnt as f64;
        let coin0_bal = helper.coin_balance(&test_coins[0], user) as f64;
        let coin1_bal = helper.coin_balance(&test_coins[1], user) as f64;

        if (coin0_amnt - coin0_bal) / 1e6 > tolerance * cnt as f64 {
            assert_eq!(coin0_amnt, coin0_bal, "Coin0 balances mismatch");
        }
        if (coin1_amnt - coin1_bal) / 1e10 > tolerance * cnt as f64 {
            assert_eq!(coin1_amnt, coin1_bal, "Coin1 balances mismatch");
        }
    }
}

#[test]
fn single_test() {
    // Test variables
    let case = [
        (0, 1000000, 0),
        (0, 202533073667, 23),
        (0, 244561165884, 202),
        (0, 627663051239, 5),
        (1, 825380672210, 0),
        (0, 340307162025, 226),
        (0, 797530352417, 0),
        (0, 873463538933, 0),
        (1, 69117398807, 0),
        (1, 440491035866, 0),
        (1, 904956772394, 0),
        (0, 1000000, 0),
        (0, 511549452379, 11),
        (1, 1000000, 0),
        (1, 1000000, 0),
        (0, 1000000, 0),
        (0, 1000000, 0),
        (0, 1000000, 0),
        (0, 1109590, 905),
        (0, 868215889609, 3259),
        (1, 747316083156, 390),
        (0, 188799176698, 2844),
        (0, 716600262745, 1471),
        (0, 280870258562, 1688),
        (0, 24061140662, 1729),
        (0, 293934332363, 592),
        (1, 647011923355, 1339),
        (1, 944578706272, 372),
        (0, 310432124606, 2798),
        (1, 630211682144, 1187),
        (1, 596382670017, 1475),
        (0, 311010946277, 1665),
        (0, 600216065773, 1527),
        (0, 694120684530, 2868),
        (0, 838465593240, 501),
        (0, 845345955677, 2387),
    ];

    simulate_case(case.to_vec());
}

#[test]
fn single_provide_test() {
    let case = [
        ("bbb", 287166875150, 216951545941),
        ("bbb", 671353776007, 92309496809),
        ("bbb", 490534003722, 640604342342),
        ("bbb", 208423623268, 267950669874),
        ("bbb", 717061608586, 728344078152),
        ("bbb", 579995009807, 557225637539),
        ("bbb", 745569605635, 568909166207),
        ("bbb", 371145293172, 89225008921),
        ("bbb", 304228471669, 419036924501),
        ("bbb", 481757145539, 559544927040),
        ("bbb", 654615636767, 768315025971),
    ];

    simulate_provide_case(case.to_vec());
}

fn generate_cases() -> impl Strategy<Value = Vec<(usize, u128, u64)>> {
    prop::collection::vec(
        (
            0..=1usize,                     // offer_ind
            1_000000..1_000_000_000000u128, // dy
            0..3600u64,                     // shift_time
        ),
        0..MAX_EVENTS,
    )
}

fn generate_provide_cases() -> impl Strategy<Value = Vec<(String, u128, u128)>> {
    prop::collection::vec(
        (
            "[a-b]{3}",                     // user
            1_000000..1_000_000_000000u128, // coin0
            1_000000..1_000_000_000000u128, // coin1
        ),
        MAX_EVENTS,
    )
}

proptest! {
    // #[ignore]
    #[test]
    fn simulate_transactions(case in generate_cases()) {
        simulate_case(case);
    }
}

proptest! {
    #[ignore]
    #[test]
    fn simulate_provide_withdraw(case in generate_provide_cases()) {
        simulate_provide_case(case);
    }
}
