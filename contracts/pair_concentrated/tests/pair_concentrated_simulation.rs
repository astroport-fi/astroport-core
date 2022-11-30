extern crate core;

mod helper;

use crate::helper::{f64_to_dec, AppExtension, Helper, TestCoin};
use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_concentrated::error::ContractError;
use cosmwasm_std::{Addr, Decimal};
use proptest::prelude::*;

const MAX_EVENTS: usize = 100;

fn simulate_case(case: Vec<(impl Into<String>, usize, u128, u64)>) {
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

    let balances = vec![100_000_000_000000u128, 100_000_000_000000u128];

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(balances[0]),
        helper.assets[&test_coins[1]].with_balance(balances[1]),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let mut i = 0;
    for (user_addr, offer_ind, dy, shift_time) in case {
        let _ask_ind = 1 - offer_ind;

        let user = Addr::unchecked(user_addr);
        println!("{user}, {offer_ind} {dy} {shift_time}");
        let offer_asset = helper.assets[&test_coins[offer_ind]].with_balance(dy);
        // let balance_before = helper.coin_balance(&test_coins[ask_ind], &user);
        helper.give_me_money(&[offer_asset.clone()], &user);
        if let Err(err) = helper.swap(&user, &offer_asset, None) {
            let err: ContractError = err.downcast().unwrap();
            match err {
                ContractError::MaxSpreadAssertion {} => {
                    // if swap fails because of spread then skip this case
                    println!("i: {i} offer_ind: {offer_ind} dy: {dy} - exceeds spread limit");
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

#[test]
fn single_test() {
    // Test variables
    let case = [
        ("aaaaaaaaaa", 1, 7836860099, 0),
        ("aaaaaaaaaa", 0, 91908739947, 2337),
        ("aaaaaaaaaa", 0, 1000000, 73),
        ("aaaaaaaaaa", 0, 1000000, 0),
        ("aaaaaaaaaa", 0, 1000000, 0),
    ];

    simulate_case(case.to_vec());
}

fn generate_cases() -> impl Strategy<Value = Vec<(String, usize, u128, u64)>> {
    prop::collection::vec(
        (
            "[a-z]{10}",                    // user
            0..=1usize,                     // offer_ind
            1_000000..1_000_000_000000u128, // dy
            0..3600u64,                     // shift_time
        ),
        0..MAX_EVENTS,
    )
}

proptest! {
    #[ignore]
    #[test]
    fn simulate_transactions(case in generate_cases()) {
        simulate_case(case);
    }
}
