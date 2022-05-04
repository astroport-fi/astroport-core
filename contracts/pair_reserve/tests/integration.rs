use cosmwasm_std::{Addr, Decimal, Uint128};
use terra_multi_test::Executor;

use astroport::asset::{Asset, AssetInfo};
use astroport::pair_reserve::{
    ConfigResponse, ExecuteMsg, FlowParams, PoolParams, PoolResponse, QueryMsg, UpdateFlowParams,
    UpdateParams,
};

use crate::test_utils::{mock_app, Helper};
use crate::test_utils::{AssetExt, AssetsExt, TerraAppExtension};

#[cfg(test)]
mod test_utils;

#[test]
fn test_config_update() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut router, &owner);

    let mut params = UpdateParams {
        entry: Some(UpdateFlowParams {
            base_pool: Uint128::zero(),
            min_spread: 0,
            recovery_period: 0,
        }),
        exit: None,
    };

    let err = router
        .execute_contract(
            Addr::unchecked("anyone"),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(&err.to_string(), "Unauthorized");

    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: base_pool cannot be zero"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.base_pool = Uint128::from(1000u128);
        flow
    });

    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: Min spread must be within [1, 10000] limit"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.min_spread = 500;
        flow
    });
    let err = router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Inflow validation error: Recovery period cannot be zero"
    );

    params.entry = params.entry.map(|mut flow| {
        flow.recovery_period = 100;
        flow
    });
    router
        .execute_contract(
            owner.clone(),
            helper.pair.clone(),
            &ExecuteMsg::UpdatePoolParameters {
                params: params.clone(),
            },
            &[],
        )
        .unwrap();
    let config = helper.get_config(&mut router).unwrap();
    let need_params = PoolParams {
        entry: FlowParams {
            base_pool: Uint128::from(1000u128),
            min_spread: 500,
            recovery_period: 100,
            pool_delta: Decimal::zero(),
        },
        exit: FlowParams {
            base_pool: Uint128::from(100_000_000_000000u128),
            min_spread: 100,
            recovery_period: 100,
            pool_delta: Decimal::zero(),
        },
        last_repl_block: router.block_info().height,
        oracles: helper.oracles.clone(),
    };
    assert_eq!(config.pool_params, need_params);

    // Checking update_whitelist() and update_oracles() in the same way as they use the same underlying logic
    for func in [Helper::update_whitelist, Helper::update_oracles] {
        let err = func(&helper, &mut router, "owner", vec![], vec![]).unwrap_err();
        assert_eq!(
            &err.to_string(),
            "Generic error: Append and remove arrays are empty"
        );
        func(
            &helper,
            &mut router,
            "owner",
            vec!["alice", "bob", "john"],
            vec![],
        )
        .unwrap();
        // Alice is already in the list
        let err = func(&helper, &mut router, "owner", vec!["alice"], vec![]).unwrap_err();
        assert_eq!(
            &err.to_string(),
            "Generic error: Append and remove arrays are empty"
        );
        // Random_addr is not in the list thus it cannot be removed
        let err = func(&helper, &mut router, "owner", vec![], vec!["random_addr"]).unwrap_err();
        assert_eq!(
            &err.to_string(),
            "Generic error: Append and remove arrays are empty"
        );
        func(&helper, &mut router, "owner", vec![], vec!["john"]).unwrap();
    }

    let list = vec![Addr::unchecked("alice"), Addr::unchecked("bob")];
    let config = helper.get_config(&mut router).unwrap();
    assert_eq!(config.providers_whitelist, list);
    let mut oracles = helper.oracles.clone();
    oracles.extend(list);
    assert_eq!(config.pool_params.oracles, oracles)
}

#[test]
fn test_oracles() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    // Filling up the LP pool
    helper
        .update_whitelist(&mut app, "owner", vec!["owner"], vec![])
        .unwrap();
    let lp_assets = helper.assets.with_balances(1000_000000, 0);
    helper
        .provide_liquidity(&mut app, "owner", lp_assets, None)
        .unwrap();

    let ust_asset = helper.assets[1].with_balance(20_000000);
    helper.give_coins(&mut app, "user", &ust_asset);

    // Removing initial oracles
    let oracles = helper.oracles.iter().map(|addr| addr.as_str()).collect();
    helper
        .update_oracles(&mut app, "owner", vec![], oracles)
        .unwrap();

    let err = helper
        .native_swap(&mut app, "user", &ust_asset, true)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Failed to retrieve the asset price from the oracles"
    );

    // Set one oracle
    helper
        .update_oracles(&mut app, "owner", vec![helper.oracles[0].as_str()], vec![])
        .unwrap();
    // Now swap should work
    helper
        .native_swap(&mut app, "user", &ust_asset, true)
        .unwrap();
}

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
                contract_addr: helper.cw20_token.clone(),
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

#[test]
fn check_swap() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    // Filling up the LP pool
    helper
        .update_whitelist(&mut app, "owner", vec!["owner"], vec![])
        .unwrap();
    let lp_assets = helper.assets.with_balances(1000_000000, 0);
    helper
        .provide_liquidity(&mut app, "owner", lp_assets, None)
        .unwrap();

    let assets = helper.assets.with_balances(1, 20000_000000);
    helper.give_coins(&mut app, "user", &assets[0]);

    let err = helper
        .native_swap(&mut app, "user", &assets[0], false)
        .unwrap_err();
    assert_eq!(&err.to_string(), "Unauthorized");

    // There is no ust in the pool as there were no swaps yet
    let err = helper.cw20_swap(&mut app, "user", &assets[0]).unwrap_err();
    assert_eq!(&err.to_string(), "Ask pool is empty");

    let err = helper
        .native_swap(&mut app, "user", &assets[1], false)
        .unwrap_err();
    assert_eq!(
        &err.to_string(),
        "Generic error: Native token balance mismatch between the argument and the transferred"
    );

    helper.give_coins(&mut app, "user", &assets[1]);
    let ust_balance = app.wrap().query_balance("user", "uusd").unwrap();
    // 20k ust + 1.39 ust tax fee
    assert_eq!(ust_balance.amount.u128(), 20001_390000);
    helper
        .native_swap(&mut app, "user", &assets[1], true)
        .unwrap();
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "user")
        .unwrap();
    // 0.5 BTC - spread fee
    assert_eq!(btc_balance, 499751u128);
    let ust_balance = app.wrap().query_balance("user", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);

    let assets = helper.assets.with_balances(1, 5_000000_000000);
    helper.give_coins(&mut app, "rich_person", &assets[1]);
    helper
        .native_swap(&mut app, "rich_person", &assets[1], true)
        .unwrap();
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "rich_person")
        .unwrap();
    assert_eq!(btc_balance, 119_001147);
    let ust_balance = app.wrap().query_balance("rich_person", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);

    let ust_balance = app.wrap().query_balance("trader", "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);
    let btc_asset = helper.assets[0].with_balance(1_000000);
    helper.give_coins(&mut app, "trader", &btc_asset);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "trader")
        .unwrap();
    assert_eq!(btc_balance, 1_000000);
    helper.cw20_swap(&mut app, "trader", &btc_asset).unwrap();
    let ust_balance = app.wrap().query_balance("trader", "uusd").unwrap();
    // 40k$ - 0.1% spread fee - 1.39 ust tax fee
    assert_eq!(ust_balance.amount.u128(), 39598_610000);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, "trader")
        .unwrap();
    assert_eq!(btc_balance, 0);
}

#[test]
fn test_queries() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    // Filling up the LP pool
    helper
        .update_whitelist(&mut app, "owner", vec!["owner"], vec![])
        .unwrap();
    let lp_assets = helper.assets.with_balances(62500_000000, 0);
    helper
        .provide_liquidity(&mut app, "owner", lp_assets.clone(), None)
        .unwrap();

    let ust_asset = helper.assets[1].with_balance(40_000_000000u128);
    let res = helper.query_simulation(&mut app, &ust_asset).unwrap();
    assert_eq!(res.return_amount.u128(), 999500);
    assert_eq!(res.spread_amount.u128(), 20000000); // in UST
    assert_eq!(res.commission_amount.u128(), 0);

    let ust_asset = ust_asset.with_balance(2_000_000_000000u128);
    let res = helper.query_simulation(&mut app, &ust_asset).unwrap();
    assert_eq!(res.return_amount.u128(), 49_019_607);
    assert_eq!(res.spread_amount.u128(), 39_215_686274); // in UST

    let btc_asset = helper.assets[0].with_balance(1_000000u128);
    let res = helper.query_simulation(&mut app, &btc_asset).unwrap();
    assert_eq!(res.return_amount.u128(), 39600_000000_u128);
    assert_eq!(res.spread_amount.u128(), 400_000000_u128); // in UST
    assert_eq!(res.commission_amount.u128(), 0);

    let btc_asset = btc_asset.with_balance(100_000000u128);
    let res = helper.query_simulation(&mut app, &btc_asset).unwrap();
    assert_eq!(res.return_amount.u128(), 3_846_153_846154);
    assert_eq!(res.spread_amount.u128(), 153_846_153846); // in UST

    // --------------- reverse simulation -----------------

    let btc_asset = helper.assets[0].with_balance(1_000000u128);
    let res = helper
        .query_reverse_simulation(&mut app, &btc_asset)
        .unwrap();
    assert_eq!(res.offer_amount.u128(), 40_400_000000);
    assert_eq!(res.spread_amount.u128(), 400_000000);
    assert_eq!(res.commission_amount.u128(), 0);

    let ust_asset = helper.assets[1].with_balance(40_000_000000u128);
    let res = helper
        .query_reverse_simulation(&mut app, &ust_asset)
        .unwrap();
    assert_eq!(res.offer_amount.u128(), 1_000500);
    assert_eq!(res.spread_amount.u128(), 20_000000);
    assert_eq!(res.commission_amount.u128(), 0);

    let res: Vec<Asset> = app
        .wrap()
        .query_wasm_smart(
            &helper.pair,
            &QueryMsg::Share {
                amount: Uint128::from(1_000000u128),
            },
        )
        .unwrap();
    let true_share = helper.assets.with_balances(1_000000u128, 0);
    assert_eq!(res, true_share);

    let resp: PoolResponse = app
        .wrap()
        .query_wasm_smart(&helper.pair, &QueryMsg::Pool {})
        .unwrap();
    assert_eq!(resp.total_share.u128(), 62500_000000u128);
    assert_eq!(resp.assets, lp_assets)
}

#[test]
fn test_swaps_and_replenishments() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, &owner);

    // Filling up the LP pool
    helper
        .update_whitelist(&mut app, "owner", vec!["owner"], vec![])
        .unwrap();
    let lp_assets = helper.assets.with_balances(62500_000000, 0);
    helper
        .provide_liquidity(&mut app, "owner", lp_assets, None)
        .unwrap();

    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(config.pool_params.exit.pool_delta.to_string(), "0");
    assert_eq!(config.pool_params.entry.pool_delta.to_string(), "0");

    let user = "user1";
    let ust_asset = helper.assets[1].with_balance(2_000_000_000000u128);
    helper.give_coins(&mut app, user, &ust_asset);
    helper
        .native_swap(&mut app, user, &ust_asset, true)
        .unwrap();

    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    assert_eq!(btc_balance, 49_019607);
    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(config.pool_params.exit.pool_delta.to_string(), "0");
    // Entry delta was increased
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "2000000000000"
    );

    let user = "user2";
    helper.give_coins(&mut app, user, &ust_asset);
    helper
        .native_swap(&mut app, user, &ust_asset, true)
        .unwrap();

    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    // The spread was increased thus user2 received less BTC
    assert_eq!(btc_balance, 47_134238);
    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(config.pool_params.exit.pool_delta.to_string(), "0");
    // Entry delta was increased again
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "4000000000000"
    );

    // Querying the swap 2MM$ for reference after replenishment
    let ust_swap_before_repl = helper.query_simulation(&mut app, &ust_asset).unwrap();
    assert_eq!(ust_swap_before_repl.return_amount.u128(), 45_355587);

    // Increasing the pair's ust balance
    let ust_asset = helper.assets[1].with_balance(1_000_000_000_000000u128); // 1MMM$
    helper.give_coins(&mut app, helper.pair.as_str(), &ust_asset);

    let user = "user3";
    let btc_asset = helper.assets[0].with_balance(100_000000);
    helper.give_coins(&mut app, user, &btc_asset);
    helper.cw20_swap(&mut app, user, &btc_asset).unwrap();

    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 3_846_152_456154);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    assert_eq!(btc_balance, 0);
    let config = helper.get_config(&mut app).unwrap();
    // 100 btc * 40000 usd = 4MM$
    assert_eq!(
        config.pool_params.exit.pool_delta.to_string(),
        "4000000000000"
    );
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "4000000000000"
    );

    let user = "user4";
    helper.give_coins(&mut app, user, &btc_asset);
    helper.cw20_swap(&mut app, user, &btc_asset).unwrap();

    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    // Spread has been increased thus swapping the same amount of BTC gives less UST
    assert_eq!(ust_balance.amount.u128(), 3_550_075_651602);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    assert_eq!(btc_balance, 0);
    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(
        config.pool_params.exit.pool_delta.to_string(),
        "8000000000000"
    );
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "4000000000000"
    );

    // Querying the swap of 100 BTC for reference after replenishment
    let btc_swap_before_repl = helper.query_simulation(&mut app, &btc_asset).unwrap();
    assert_eq!(btc_swap_before_repl.return_amount.u128(), 3_265_432_098766);

    // Going to the next block
    app.skip_blocks(1);

    let config = helper.get_config(&mut app).unwrap();
    // Both deltas were decreased
    assert_eq!(
        config.pool_params.exit.pool_delta.to_string(),
        "7920000000000"
    );
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "3600000000000"
    );

    let user = "user5";
    let ust_asset = helper.assets[1].with_balance(2_000_000_000000u128);
    helper.give_coins(&mut app, user, &ust_asset);
    helper
        .native_swap(&mut app, user, &ust_asset, true)
        .unwrap();
    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    assert_eq!(ust_balance.amount.u128(), 0);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    // should be more than 45_355587 and less than 49_019607 (first swap without big spread)
    assert!(btc_balance > ust_swap_before_repl.return_amount.u128());
    assert_eq!(btc_balance, 45_703170);
    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(
        config.pool_params.exit.pool_delta.to_string(),
        "7920000000000"
    );
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "5600000000000"
    );

    let user = "user6";
    let btc_asset = helper.assets[0].with_balance(100_000000);
    helper.give_coins(&mut app, user, &btc_asset);
    helper.cw20_swap(&mut app, user, &btc_asset).unwrap();

    let ust_balance = app.wrap().query_balance(user, "uusd").unwrap();
    // should be more than 3_265_432_098766 and less than 3_846_152_456154 (first swap without big spread)
    assert!(ust_balance.amount.u128() > btc_swap_before_repl.return_amount.u128());
    assert_eq!(ust_balance.amount.u128(), 3_271_011_233067);
    let btc_balance = helper
        .get_token_balance(&mut app, &helper.btc_token, user)
        .unwrap();
    assert_eq!(btc_balance, 0);
    let config = helper.get_config(&mut app).unwrap();
    assert_eq!(
        config.pool_params.exit.pool_delta.to_string(),
        "11920000000000"
    );
    assert_eq!(
        config.pool_params.entry.pool_delta.to_string(),
        "5600000000000"
    );
}
