extern crate core;

mod helper;

use crate::helper::{dec_to_f64, f64_to_dec, AppExtension, Helper, TestCoin};
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
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
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

fn simulate_provide_case(case: Vec<(impl Into<String>, u128, u128, u64)>) {
    let owner = Addr::unchecked("owner");
    let tolerance = 0.05; // allowed loss per provide due to integer math withing contract

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

    // owner makes the first provide cuz the pool charges small amount of fees
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(1_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(1_000_000000u128),
    ];
    helper.provide_liquidity(&owner, &assets).unwrap();

    let mut accounts: HashMap<Addr, (u128, u128, u8)> = HashMap::new();
    for (user, coin0_amnt, coin1_amnt, shift_time) in case {
        let user = Addr::unchecked(user);
        println!("{user} {coin0_amnt} {coin1_amnt}");
        let assets = vec![
            helper.assets[&test_coins[0]].with_balance(coin0_amnt),
            helper.assets[&test_coins[1]].with_balance(coin1_amnt),
        ];
        helper.give_me_money(&assets, &user);

        if let Err(err) = helper.provide_liquidity(&user, &assets) {
            let err: ContractError = err.downcast().unwrap();
            match err {
                ContractError::MaxSpreadAssertion {} => {
                    // if swap fails because of spread then skip this case
                    println!("spread limit exceeded");
                }
                _ => panic!("{err}"),
            }
        } else {
            let entry = accounts.entry(user).or_default();
            entry.0 = entry.0 + coin0_amnt;
            entry.1 = entry.1 + coin1_amnt;
            entry.2 += 1;
        }

        // Shift time so EMA will update oracle prices
        helper.app.next_block(shift_time);
    }

    let config = helper.query_config().unwrap();
    let price_scale = dec_to_f64(config.pool_state.price_state.price_scale);

    for (user, &(coin0_amnt, coin1_amnt, cnt)) in &accounts {
        let lp_amount = helper.token_balance(&helper.lp_token, user);
        if cnt != 0 {
            helper.withdraw_liquidity(user, lp_amount, vec![]).unwrap();
        }

        let total_sent_liq = coin0_amnt as f64 + coin1_amnt as f64 * price_scale;
        let coin0_bal = helper.coin_balance(&test_coins[0], user) as f64;
        let coin1_bal = helper.coin_balance(&test_coins[1], user) as f64;
        let total_contract_liq = coin0_bal + coin1_bal * price_scale;

        if 1.0 - total_contract_liq / total_sent_liq > tolerance * cnt as f64 {
            assert_eq!(
                total_sent_liq, total_contract_liq,
                "Too much losses in {user}'s liquidity"
            );
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
        ("aaa", 1000107, 594723897570, 197),
        ("bbb", 118018421609, 866237402992, 1681),
        ("bab", 545517989124, 881646979723, 2555),
        ("bbb", 287166875150, 216951545941, 3359),
        ("abb", 124961125834, 474622062730, 2077),
        ("aaa", 15773250045, 941579741450, 1198),
        ("abb", 869290979433, 231139951269, 155),
        ("bbb", 489892656085, 470441621889, 1916),
        ("bba", 527331704654, 293938537883, 2101),
        ("bab", 397172218491, 555571280367, 1696),
        ("aba", 364154509726, 718075826094, 3092),
        ("baa", 155800416418, 537274193065, 375),
        ("aba", 519998444778, 650945767164, 3403),
        ("aba", 490025440189, 664470287970, 3451),
        ("aab", 719468877853, 589687952509, 2473),
        ("aaa", 578253806045, 378503907467, 21),
        ("bbb", 395640215157, 98817071063, 2755),
        ("aab", 371016145602, 744232303397, 323),
        ("aba", 9231411809, 563696727107, 3364),
        ("aba", 236903055947, 426256358744, 3406),
        ("aaa", 600852618399, 121961039074, 3471),
        ("aab", 326991602417, 962805514134, 1208),
        ("bab", 725067759250, 526927133600, 1477),
        ("bbb", 208423623268, 267950669874, 3036),
        ("bba", 324345682294, 917258889258, 2036),
        ("baa", 631496244660, 597148885687, 822),
        ("abb", 544603336979, 914047648485, 1878),
        ("aaa", 380540722468, 876147769404, 445),
        ("bab", 171307546213, 542606562109, 2667),
        ("aaa", 803133216637, 536888160757, 1477),
        ("aab", 798701048448, 447621664465, 2625),
        ("aaa", 529568066448, 969956360969, 922),
        ("abb", 440168549394, 366046706509, 2583),
        ("baa", 678168792654, 200020793371, 2554),
        ("bba", 872196737841, 888825256324, 2943),
        ("bbb", 400967045141, 882270262696, 157),
        ("aaa", 394343540769, 231295965597, 2376),
        ("baa", 291008197310, 489383033801, 334),
        ("bba", 748194556086, 195431639218, 2609),
        ("aaa", 672004396539, 662701988821, 1200),
        ("bbb", 598679023303, 40730083508, 342),
        ("aaa", 861995955441, 859305201622, 371),
        ("baa", 208190222301, 564405565438, 2587),
        ("bab", 535445721599, 46600163393, 1495),
        ("baa", 168786397151, 668162284987, 2161),
        ("abb", 703522158927, 148007906728, 3038),
        ("aaa", 536093534284, 808170308790, 1380),
        ("aab", 454822690791, 710185613454, 241),
        ("aab", 171701593822, 902322808409, 3064),
        ("bba", 358112911824, 91790675209, 794),
        ("bbb", 476031477866, 275184138697, 1213),
        ("aba", 968643490362, 790577622555, 2036),
        ("bba", 346500233057, 857488811527, 2496),
        ("baa", 767958745099, 881314575102, 3233),
        ("bba", 79139307223, 687075059767, 2995),
        ("aba", 773303534271, 613989708385, 2719),
        ("bba", 375228353551, 147564468426, 1027),
        ("bab", 836724995486, 148016626885, 494),
        ("aba", 272022060743, 583596491847, 1157),
        ("aba", 191821103112, 490281609793, 490),
        ("abb", 94653167899, 932786368102, 1810),
        ("bbb", 804917774813, 13775357034, 1272),
        ("abb", 56134397731, 719331741547, 2927),
        ("aba", 845287628341, 534059109177, 1904),
        ("abb", 784462231243, 154167184048, 1229),
        ("bbb", 654615636767, 768315025971, 3216),
        ("bba", 893530774682, 731616339416, 3281),
        ("bbb", 343723573837, 150290349315, 2803),
        ("abb", 22227179932, 187040634950, 2680),
        ("bab", 200637641020, 147006024706, 201),
        ("baa", 875341516868, 472241634877, 1465),
        ("bba", 256420237132, 692647182519, 2273),
        ("abb", 575966363984, 867783883393, 1324),
        ("aab", 461578271314, 497809535606, 21),
        ("abb", 828000102476, 713362572580, 846),
        ("baa", 228912071527, 28317247489, 934),
        ("aaa", 844735877718, 409278236302, 2922),
        ("aba", 187177485309, 550680536839, 3100),
        ("aba", 235519991408, 794638182512, 2673),
        ("bba", 209255529957, 854621274698, 3445),
        ("bba", 169371355699, 767915066308, 690),
        ("bbb", 294038932236, 612820830935, 299),
        ("bab", 574221615498, 188638677434, 777),
        ("aab", 615793637311, 525031135192, 2167),
        ("aab", 437870178814, 947454396380, 3211),
        ("aab", 465240818778, 355520463158, 566),
        ("bbb", 113520975489, 266019523208, 1228),
        ("baa", 255011587436, 157170193250, 2527),
        ("aba", 150715871611, 140659656729, 3394),
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

fn generate_provide_cases() -> impl Strategy<Value = Vec<(String, u128, u128, u64)>> {
    prop::collection::vec(
        (
            "[a-b]{3}",                     // user
            1_000000..1_000_000_000000u128, // coin0
            1_000000..1_000_000_000000u128, // coin1
            0..3600u64,                     // shift_time
        ),
        MAX_EVENTS,
    )
}

proptest! {
    #[ignore]
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
