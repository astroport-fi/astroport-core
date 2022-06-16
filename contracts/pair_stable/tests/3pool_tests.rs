use astroport_pair_stable::error::ContractError;
use cosmwasm_std::Addr;

use crate::helper::AssetInfoExt;
use crate::helper::{Helper, TestCoin};

mod helper;

#[test]
fn provide_and_withdraw_no_fee() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native("uluna"),
        TestCoin::cw20("USDC"),
        TestCoin::cw20("USDD"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone(), 100u64).unwrap();

    let user1 = Addr::unchecked("user1");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user1);

    helper.provide_liquidity(&user1, &assets).unwrap();

    assert_eq!(299_996666, helper.token_balance(&helper.lp_token, &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user1));
    assert_eq!(0, helper.coin_balance(&test_coins[2], &user1));

    // The user2 with the same assets should receive the same share
    let user2 = Addr::unchecked("user2");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user2);
    helper.provide_liquidity(&user2, &assets).unwrap();
    assert_eq!(299_996667, helper.token_balance(&helper.lp_token, &user2));

    // The user3 makes imbalanced provide thus he is charged with fees
    let user3 = Addr::unchecked("user3");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(200_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user3);
    helper.provide_liquidity(&user3, &assets).unwrap();
    assert_eq!(299_995417, helper.token_balance(&helper.lp_token, &user3));

    // Providing last asset with explicit zero amount should give the same result
    let user4 = Addr::unchecked("user4");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(200_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(0),
    ];
    helper.give_me_money(&assets, &user4);
    helper.provide_liquidity(&user4, &assets).unwrap();
    assert_eq!(299_993473, helper.token_balance(&helper.lp_token, &user4));

    helper
        .withdraw_liquidity(&user1, 299_996666, vec![])
        .unwrap();

    assert_eq!(0, helper.token_balance(&helper.lp_token, &user1));
    // Previous imbalanced provides resulted in different share in assets
    assert_eq!(150_000555, helper.coin_balance(&test_coins[0], &user1));
    assert_eq!(100_000370, helper.coin_balance(&test_coins[1], &user1));
    assert_eq!(50_000185, helper.coin_balance(&test_coins[2], &user1));

    // Checking imbalanced withdraw. Withdrawing only the first asset x 300 with the whole amount of LP tokens
    helper
        .withdraw_liquidity(
            &user2,
            299_996667,
            vec![helper.assets[&test_coins[0]].with_balance(300_000000)],
        )
        .unwrap();

    assert_eq!(2098, helper.token_balance(&helper.lp_token, &user2));
    assert_eq!(300_000000, helper.coin_balance(&test_coins[0], &user2));
    assert_eq!(0, helper.coin_balance(&test_coins[1], &user2));
    assert_eq!(0, helper.coin_balance(&test_coins[2], &user2));

    // Trying to receive more than possible
    let err = helper
        .withdraw_liquidity(
            &user3,
            100_000000,
            vec![helper.assets[&test_coins[1]].with_balance(101_000000)],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: Not enough LP tokens. You need 100997798 LP tokens.",
        err.root_cause().to_string()
    );

    // Providing more LP tokens than needed. The rest will be kept on the user's balance
    helper
        .withdraw_liquidity(
            &user3,
            200_997798,
            vec![helper.assets[&test_coins[1]].with_balance(101_000000)],
        )
        .unwrap();

    // initial balance - spent amount; 100 goes back to the user3
    assert_eq!(
        299_995417 - 100_997798,
        helper.token_balance(&helper.lp_token, &user3)
    );
    assert_eq!(0, helper.coin_balance(&test_coins[0], &user3));
    assert_eq!(101_000000, helper.coin_balance(&test_coins[1], &user3));
    assert_eq!(0, helper.coin_balance(&test_coins[2], &user3));
}

#[test]
fn check_wrong_initializations() {
    let owner = Addr::unchecked("owner");

    let err = Helper::new(&owner, vec![TestCoin::native("uluna")], 100u64).unwrap_err();

    assert_eq!(
        ContractError::InvalidNumberOfAssets {},
        err.downcast().unwrap()
    );

    let err = Helper::new(
        &owner,
        vec![
            TestCoin::native("one"),
            TestCoin::cw20("two"),
            TestCoin::native("three"),
            TestCoin::cw20("four"),
            TestCoin::native("five"),
            TestCoin::cw20("six"),
        ],
        100u64,
    )
    .unwrap_err();

    assert_eq!(
        ContractError::InvalidNumberOfAssets {},
        err.downcast().unwrap()
    );

    let err = Helper::new(
        &owner,
        vec![
            TestCoin::native("uluna"),
            TestCoin::native("uluna"),
            TestCoin::cw20("USDC"),
        ],
        100u64,
    )
    .unwrap_err();

    assert_eq!(ContractError::DoublingAssets {}, err.downcast().unwrap());

    // 5 assets in the pool is okay
    Helper::new(
        &owner,
        vec![
            TestCoin::native("one"),
            TestCoin::cw20("two"),
            TestCoin::native("three"),
            TestCoin::cw20("four"),
            TestCoin::native("five"),
        ],
        100u64,
    )
    .unwrap();
}
