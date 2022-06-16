use astroport_pair_stable::error::ContractError;
use cosmwasm_std::Addr;
use cw20::{BalanceResponse, Cw20Contract, Cw20QueryMsg};

use crate::helper::AssetInfoExt;
use crate::helper::{Helper, TestCoin};

mod helper;

#[test]
fn provide_works() {
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

    let lp_balance = helper.token_balance(&helper.lp_token, &user1);
    let token1_balance = helper.coin_balance(&test_coins[0], &user1);
    let token2_balance = helper.coin_balance(&test_coins[0], &user1);
    let token3_balance = helper.coin_balance(&test_coins[0], &user1);

    assert_eq!(299_996666, lp_balance.u128());
    assert_eq!(0, token1_balance.u128());
    assert_eq!(0, token2_balance.u128());
    assert_eq!(0, token3_balance.u128());

    // The user2 with the same assets should receive the same share
    let user2 = Addr::unchecked("user2");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user2);
    helper.provide_liquidity(&user2, &assets).unwrap();
    let lp_balance = helper.token_balance(&helper.lp_token, &user2);
    assert_eq!(299_996667, lp_balance.u128());

    // The user3 makes imbalanced provide thus he is charged with fees
    let user3 = Addr::unchecked("user3");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(200_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user3);
    helper.provide_liquidity(&user3, &assets).unwrap();
    let lp_balance = helper.token_balance(&helper.lp_token, &user3);
    assert_eq!(299_995417, lp_balance.u128());

    // Providing last asset with explicit zero amount should give the same result
    let user4 = Addr::unchecked("user4");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(200_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(0),
    ];
    helper.give_me_money(&assets, &user4);
    helper.provide_liquidity(&user4, &assets).unwrap();
    let lp_balance = helper.token_balance(&helper.lp_token, &user4);
    assert_eq!(299_993473, lp_balance.u128());
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
