use astroport::asset::AssetInfoExt;
use cosmwasm_std::{Addr, Uint128};
use sim::model::{ConcentratedPairModel, A_MUL, MUL_E18};

use astroport::pair_concentrated::ConcentratedPoolParams;

use crate::helper::{AppExtension, Helper, TestCoin};

mod helper;

use proptest::prelude::*;

const MULTIPLIER_U128: u128 = 1e18 as u128;
const TEST_TOLERANCE: u128 = 0;
const MAX_EVENTS: usize = 100;

fn simulate_case(case: Vec<(impl Into<String>, usize, u128, u64)>) {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("USDC")];

    let params = ConcentratedPoolParams {
        amp: 100,
        gamma: (0.000145 * MUL_E18 as f64) as u128,
        mid_fee: (0.0005 * 1e10) as u128,
        out_fee: (0.0005 * 1e10) as u128,
        fee_gamma: 1,
        allowed_extra_profit: 0,
        adjustment_step: (0.000146 * 1e18) as u128,
        ma_half_time: 600,
    };

    let balances = vec![100_000_000_000000u128, 100_000_000_000000u128];

    // Initialize python model
    let model = ConcentratedPairModel::new(
        4 * params.amp * A_MUL,
        params.gamma,
        balances.clone(),
        2,
        vec![MUL_E18, MUL_E18],
        params.mid_fee as f64 / 1e10,
        params.out_fee as f64 / 1e10,
        params.fee_gamma,
        params.adjustment_step as f64 / 1e18,
        params.ma_half_time,
    )
    .unwrap();

    let mut helper = Helper::new(&owner, test_coins.clone(), params).unwrap();

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(balances[0]),
        helper.assets[&test_coins[1]].with_balance(balances[1]),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let mut i = 0;
    for (user_addr, offer_ind, dy, shift_time) in case {
        let ask_ind = 1 - offer_ind;

        // Astroport
        let user = Addr::unchecked(user_addr);
        let offer_asset = helper.assets[&test_coins[offer_ind]].with_balance(dy);
        let balance_before = helper.coin_balance(&test_coins[ask_ind], &user);
        helper.give_me_money(&[offer_asset.clone()], &user);
        helper.swap(&user, &offer_asset).unwrap();
        let swap_amount = helper.coin_balance(&test_coins[ask_ind], &user) - balance_before;

        // Baseline
        let dx: f64 = model.call("sell", (dy, ask_ind, offer_ind)).unwrap();
        let dx = dx as u128;
        let price = dy * MULTIPLIER_U128 / dx;
        let _: u128 = model
            .call(
                "tweak_price",
                (
                    helper.app.block_info().time.seconds(),
                    offer_ind,
                    ask_ind,
                    price,
                ),
            )
            .unwrap();

        println!("contract: {swap_amount} model: {dx}");

        let price_state = helper.query_config().unwrap().pool_state.price_state;

        // Check price scale
        let contract_price: Uint128 = price_state.price_scale.try_into().unwrap();
        let model_prices: Vec<u128> = model.get_attr_curve("p").unwrap();
        if contract_price.u128().abs_diff(model_prices[1]) > TEST_TOLERANCE {
            assert_eq!(
                contract_price.u128(),
                model_prices[1],
                "Price scale mismatch: i: {i} contract: {contract_price}, model: {}",
                model_prices[1]
            );
        }

        // Check last price
        let contract_price: Uint128 = price_state.last_prices.try_into().unwrap();
        let model_prices: Vec<u128> = model.get_attr("last_price").unwrap();
        let diff = contract_price.u128().abs_diff(model_prices[1]);
        if diff > TEST_TOLERANCE {
            assert_eq!(
                contract_price.u128(),
                model_prices[1],
                "Last price mismatch: i: {i} contract: {contract_price}, model: {}, diff: {diff}",
                model_prices[1]
            );
        }

        // Check oracle price
        let contract_price: Uint128 = price_state.price_oracle.try_into().unwrap();
        let model_prices: Vec<u128> = model.get_attr("price_oracle").unwrap();
        let diff = contract_price.u128().abs_diff(model_prices[1]);
        if diff > TEST_TOLERANCE {
            assert_eq!(
                contract_price.u128(),
                model_prices[1],
                "Oracle price mismatch: i: {i} contract: {contract_price}, model: {}, diff: {diff}",
                model_prices[1]
            );
        }

        let diff = dx.abs_diff(swap_amount);
        if diff > TEST_TOLERANCE {
            assert_eq!(dx, swap_amount, "i: {i} offer: {offer_ind}  dy: {dy} model: {dx} contract: {swap_amount} shift: {shift_time} diff: {diff}");
        }

        i += 1;

        // Shift time so EMA will update oracle prices
        helper.app.next_block(shift_time);
    }
}

#[test]
fn single_test() {
    // Test variables
    let case = [
        ("aaaaaaaaaa", 0, 1000000, 299),
        ("aaaaaaaaaa", 0, 1000000, 460),
        ("aaaaaaaaaa", 0, 1000000, 1),
    ];

    simulate_case(case.to_vec());
}

fn generate_cases() -> impl Strategy<Value = Vec<(String, usize, u128, u64)>> {
    prop::collection::vec(
        (
            "[a-z]{10}",                    // user
            0..=1usize,                     // offer_ind
            1_000000..1_000_000_000000u128, // dy
            1..3600u64,                     // shift_time
        ),
        0..MAX_EVENTS,
    )
}

proptest! {
    #[test]
    fn simulate_transactions(case in generate_cases()) {
        simulate_case(case);
    }
}
