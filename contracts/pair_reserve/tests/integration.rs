use cosmwasm_std::Addr;

use crate::test_utils::AssetsExt;
use crate::test_utils::{mock_app, Helper};

#[warn(unused_imports)]
mod test_utils;

#[test]
fn test_provide_liquidity() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut router, &owner);

    let assets = helper.assets.with_balances(100, 0);
    let err = helper
        .provide_liquidity(&mut router, "user", assets.clone(), None)
        .unwrap_err();
    // user is not in the whitelist
    assert_eq!(&err.to_string(), "Unauthorized");

    helper
        .update_whitelist(&mut router, "owner", vec!["user"], vec![])
        .unwrap();
    let err = helper
        .provide_liquidity(&mut router, "user", assets.clone(), None)
        .unwrap_err();
    // User does not have enough coins
    assert_eq!(&err.to_string(), "Overflow: Cannot Sub with 0 and 100");

    helper.give_coin(&mut router, "user", &assets[0]);
    helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap();
    let lp_balance = helper.get_lp_balance(&mut router, "user").unwrap();
    assert_eq!(lp_balance, 100u128);
}
