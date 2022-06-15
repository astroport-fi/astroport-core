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

    let mut helper = Helper::new(&owner, test_coins.clone(), 100u64);

    let user = Addr::unchecked("user");
    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000000),
        helper.assets[&test_coins[1]].with_balance(100_000000),
        helper.assets[&test_coins[2]].with_balance(100_000000),
    ];
    helper.give_me_money(&assets, &user);

    helper.provide_liquidity(&user, &assets).unwrap();

    let balance = helper.token_balance(&helper.lp_token, &user);

    assert_eq!(299_996666, balance.u128());
}
