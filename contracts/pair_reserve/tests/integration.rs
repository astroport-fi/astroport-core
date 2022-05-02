use cosmwasm_std::Addr;
use terra_multi_test::Executor;

use astroport::asset::{Asset, AssetInfo};
use astroport::pair_reserve::{ConfigResponse, ExecuteMsg, QueryMsg};

use crate::test_utils::AssetsExt;
use crate::test_utils::{mock_app, Helper};

#[cfg(test)]
mod test_utils;

#[test]
fn test_liquidity_operations() {
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

    helper.give_coins(&mut router, "user", &assets[0]);
    helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 100u128);

    let assets = helper.assets.with_balances(50, 0);
    helper.give_coins(&mut router, "user", &assets[0]);
    helper
        .provide_liquidity(&mut router, "user", assets, Some("user2"))
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 100u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 50u128);

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Default::default(),
        },
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Default::default(),
        },
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Reserve pool accepts (native token, CW20 token) pairs only"
    );

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "ibc/uusd".to_string(),
            },
            amount: Default::default(),
        },
        helper.assets[0].clone(),
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Generic error: IBC tokens are forbidden");

    let assets = [
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Default::default(),
        },
        Asset {
            info: AssetInfo::Token {
                contract_addr: helper.astro_token.clone(),
            },
            amount: Default::default(),
        },
    ];
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Provided token does not belong to the pair"
    );

    let assets = helper.assets.with_balances(0, 1000);
    helper.give_coins(&mut router, "user", &assets[1]);
    let err = helper
        .provide_liquidity(&mut router, "user", assets, None)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Event of zero transfer");

    helper
        .withdraw_liquidity(&mut router, "user", 60u128)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 40u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.btc_token, "user")
        .unwrap();
    assert_eq!(lp_balance, 60u128);

    let err = helper
        .withdraw_liquidity(&mut router, "user2", 51u128)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Overflow: Cannot Sub with 50 and 51");

    helper
        .withdraw_liquidity(&mut router, "user2", 50u128)
        .unwrap();
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.lp_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 0u128);
    let lp_balance = helper
        .get_token_balance(&mut router, &helper.btc_token, "user2")
        .unwrap();
    assert_eq!(lp_balance, 50u128);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = app
        .execute_contract(Addr::unchecked("not_owner"), helper.pair.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.pair.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(Addr::unchecked("owner"), helper.pair.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.pair.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.pair.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app.wrap().query_wasm_smart(&helper.pair, &msg).unwrap();

    assert_eq!(res.owner, new_owner)
}
