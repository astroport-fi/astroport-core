extern crate core;

mod helper;

use crate::helper::{f64_to_dec, AppExtension, Helper, TestCoin};
use astroport::asset::AssetInfoExt;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_concentrated::error::ContractError;
use cosmwasm_std::{Addr, Decimal};
use proptest::prelude::*;

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

#[test]
fn single_test() {
    // Test variables
    let case = [
        (0, 64507528343, 0),
        (0, 268464813395, 0),
        (0, 288920297765, 0),
        (1, 577544148238, 157),
        (0, 612745575482, 55),
        (1, 956885767962, 0),
        (0, 620816500741, 0),
        (0, 492108192373, 0),
        (0, 960714428857, 0),
        (1, 205497441195, 2),
        (1, 770423070121, 8),
        (1, 440491035866, 19),
        (1, 904956772394, 0),
        (0, 780669112290, 0),
        (0, 952991709839, 0),
        (1, 879615457608, 21),
        (1, 813258681300, 0),
        (0, 696619135227, 0),
        (0, 737348078240, 414),
        (1, 549010866741, 185),
        (1, 459655474981, 905),
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
        (1, 944245254417, 930),
        (0, 838465593240, 501),
        (0, 424529795484, 3256),
    ];

    simulate_case(case.to_vec());
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

proptest! {
    #[ignore]
    #[test]
    fn simulate_transactions(case in generate_cases()) {
        simulate_case(case);
    }
}
