#![cfg(not(tarpaulin_include))]

use astroport::asset::{AssetInfo, AssetInfoExt};
use astroport::maker::{
    AssetWithLimit, Config, ExecuteMsg, PoolRoute, QueryMsg, RouteStep, MAX_SWAPS_DEPTH,
};
use astroport_maker::error::ContractError;
use astroport_test::cw_multi_test::Executor;
use cosmwasm_std::{coin, Addr, Uint128};
use itertools::Itertools;

use crate::common::helper::{Helper, ASTRO_DENOM};

mod common;

#[test]
fn check_set_routes() {
    let mut helper = Helper::new().unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, ASTRO_DENOM),
        ])
        .unwrap();

    let pool_1 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "ucoin"),
            coin(1_000_000_000000, "uusd"),
        ])
        .unwrap();

    // Set wrong pool addr
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("ucoin"),
            asset_out: AssetInfo::native("uusd"),
            pool_addr: astro_pair.contract_addr.to_string(),
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidPoolAsset {
            pool_addr: astro_pair.contract_addr.to_string(),
            asset: "ucoin".to_string()
        }
    );
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("ucoin"),
            asset_out: AssetInfo::native("rand"),
            pool_addr: pool_1.contract_addr.to_string(),
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidPoolAsset {
            pool_addr: pool_1.contract_addr.to_string(),
            asset: "rand".to_string()
        }
    );

    // ucoin -> uusd -> astro
    helper
        .set_pool_routes(vec![
            PoolRoute {
                asset_in: AssetInfo::native("ucoin"),
                asset_out: AssetInfo::native("uusd"),
                pool_addr: pool_1.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("uusd"),
                asset_out: AssetInfo::native(ASTRO_DENOM),
                pool_addr: astro_pair.contract_addr.to_string(),
            },
        ])
        .unwrap();

    let route = helper.query_route("ucoin").unwrap();
    assert_eq!(
        route,
        vec![
            RouteStep {
                pool_addr: pool_1.contract_addr.clone(),
                asset_out: AssetInfo::native("uusd"),
            },
            RouteStep {
                asset_out: AssetInfo::native(ASTRO_DENOM),
                pool_addr: astro_pair.contract_addr.clone(),
            }
        ]
    );

    let pool_2 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "utest"),
            coin(1_000_000_000000, "uusd"),
        ])
        .unwrap();

    //          utest
    //            |
    // ucoin -> uusd -> astro
    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("utest"),
            asset_out: AssetInfo::native("uusd"),
            pool_addr: pool_2.contract_addr.to_string(),
        }])
        .unwrap();

    let route = helper.query_route("utest").unwrap();
    assert_eq!(
        route,
        vec![
            RouteStep {
                pool_addr: pool_2.contract_addr.clone(),
                asset_out: AssetInfo::native("uusd"),
            },
            RouteStep {
                asset_out: AssetInfo::native(ASTRO_DENOM),
                pool_addr: astro_pair.contract_addr.clone(),
            }
        ]
    );

    let pool_3 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "utest"),
            coin(1_000_000_000000, "ucoin"),
        ])
        .unwrap();

    // Update route
    //  utest
    //    |
    // ucoin -> uusd -> astro
    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("utest"),
            asset_out: AssetInfo::native("ucoin"),
            pool_addr: pool_3.contract_addr.to_string(),
        }])
        .unwrap();

    let route = helper.query_route("utest").unwrap();
    assert_eq!(
        route,
        vec![
            RouteStep {
                pool_addr: pool_3.contract_addr.clone(),
                asset_out: AssetInfo::native("ucoin"),
            },
            RouteStep {
                pool_addr: pool_1.contract_addr.clone(),
                asset_out: AssetInfo::native("uusd"),
            },
            RouteStep {
                asset_out: AssetInfo::native(ASTRO_DENOM),
                pool_addr: astro_pair.contract_addr.clone(),
            }
        ]
    );

    let pool_4 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "utest"),
            coin(1_000_000_000000, "uatom"),
        ])
        .unwrap();

    // Trying to set route which doesn't lead to ASTRO
    //  utest -> uatom
    //    x
    // ucoin -> uusd -> astro
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("utest"),
            asset_out: AssetInfo::native("uatom"),
            pool_addr: pool_4.contract_addr.to_string(),
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::RouteNotFound {
            asset: "uatom".to_string(),
        }
    );

    // Checking long swap path
    let mut routes = (0..=MAX_SWAPS_DEPTH)
        .into_iter()
        .tuple_windows()
        .map(|(i, j)| {
            let coin_a = format!("coin{i}");
            let coin_b = format!("coin{j}");
            let pool_addr = helper
                .create_and_seed_pair([
                    coin(1_000_000_000000, &coin_a),
                    coin(1_000_000_000000, &coin_b),
                ])
                .unwrap();
            PoolRoute {
                asset_in: AssetInfo::native(coin_a),
                asset_out: AssetInfo::native(coin_b),
                pool_addr: pool_addr.contract_addr.to_string(),
            }
        })
        .collect_vec();

    let last_coin = format!("coin{MAX_SWAPS_DEPTH}");

    let pool_addr = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, &last_coin),
            coin(1_000_000_000000, ASTRO_DENOM),
        ])
        .unwrap();

    routes.push(PoolRoute {
        asset_in: AssetInfo::native(last_coin),
        asset_out: AssetInfo::native(ASTRO_DENOM),
        pool_addr: pool_addr.contract_addr.to_string(),
    });

    let err = helper.set_pool_routes(routes).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::FailedToBuildRoute {
            asset: "coin0".to_string(),
        }
    );
}

#[test]
fn test_collect() {
    let mut helper = Helper::new().unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, ASTRO_DENOM),
        ])
        .unwrap();

    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("uusd"),
            asset_out: AssetInfo::native(ASTRO_DENOM),
            pool_addr: astro_pair.contract_addr.to_string(),
        }])
        .unwrap();

    // mock received fees
    let maker = helper.maker.clone();
    helper.give_me_money(
        &[AssetInfo::native("uusd").with_balance(1_000000u64)],
        &maker,
    );

    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::native("uusd"),
            limit: None,
        }])
        .unwrap();

    let uusd_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "uusd")
        .unwrap();
    assert_eq!(uusd_bal.amount.u128(), 0);

    let astro_bal = helper
        .app
        .wrap()
        .query_balance(&helper.satellite, ASTRO_DENOM)
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 997799);

    let pool_1 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "coin_a"),
            coin(1_000_000_000000, "uusd"),
        ])
        .unwrap();
    let pool_2 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "coin_a"),
            coin(1_000_000_000000, "coin_b"),
        ])
        .unwrap();
    let pool_3 = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "coin_c"),
            coin(1_000_000_000000, "uusd"),
        ])
        .unwrap();

    // Set routes
    //                     coin_c
    //                      |
    // coin_b -> coin_a -> uusd -> astro
    helper
        .set_pool_routes(vec![
            PoolRoute {
                asset_in: AssetInfo::native("coin_a"),
                asset_out: AssetInfo::native("uusd"),
                pool_addr: pool_1.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("coin_b"),
                asset_out: AssetInfo::native("coin_a"),
                pool_addr: pool_2.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("coin_c"),
                asset_out: AssetInfo::native("uusd"),
                pool_addr: pool_3.contract_addr.to_string(),
            },
        ])
        .unwrap();

    helper.give_me_money(
        &[AssetInfo::native("coin_a").with_balance(1_000000u64)],
        &maker,
    );
    helper.give_me_money(
        &[AssetInfo::native("coin_b").with_balance(1_000000u64)],
        &maker,
    );
    helper.give_me_money(
        &[AssetInfo::native("coin_c").with_balance(1_000000u64)],
        &maker,
    );

    helper
        .collect(vec![
            AssetWithLimit {
                info: AssetInfo::native("coin_a"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("coin_b"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("coin_c"),
                limit: None,
            },
        ])
        .unwrap();

    let coin_a_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_a")
        .unwrap();
    assert_eq!(coin_a_bal.amount.u128(), 0);
    let coin_b_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_b")
        .unwrap();
    assert_eq!(coin_b_bal.amount.u128(), 0);
    let coin_c_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_c")
        .unwrap();
    assert_eq!(coin_c_bal.amount.u128(), 0);

    // Satellite has received fees converted to astro
    let astro_bal = helper
        .app
        .wrap()
        .query_balance(&helper.satellite, ASTRO_DENOM)
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 3982402);

    // Check collect with limit
    helper.give_me_money(
        &[AssetInfo::native("coin_c").with_balance(1_000000u64)],
        &maker,
    );
    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::native("coin_c"),
            limit: Some(500u128.into()),
        }])
        .unwrap();
    let coin_c_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_c")
        .unwrap();
    assert_eq!(coin_c_bal.amount.u128(), 999500);

    // Try to set limit higher than balance
    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::native("coin_c"),
            limit: Some(1_000_000u128.into()),
        }])
        .unwrap();
    let coin_c_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_c")
        .unwrap();
    assert_eq!(coin_c_bal.amount.u128(), 0);

    // query all routes
    let routes: Vec<PoolRoute> = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.maker,
            &QueryMsg::Routes {
                start_after: None,
                limit: Some(100),
            },
        )
        .unwrap();
    assert_eq!(
        routes,
        vec![
            PoolRoute {
                asset_in: AssetInfo::native("coin_a"),
                asset_out: AssetInfo::native("uusd"),
                pool_addr: pool_1.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("coin_b"),
                asset_out: AssetInfo::native("coin_a"),
                pool_addr: pool_2.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("coin_c"),
                asset_out: AssetInfo::native("uusd"),
                pool_addr: pool_3.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("uusd"),
                asset_out: AssetInfo::native(ASTRO_DENOM),
                pool_addr: astro_pair.contract_addr.to_string(),
            }
        ]
    );

    let estimated_astro_out: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.maker,
            &QueryMsg::EstimateSwap {
                asset_in: AssetInfo::native("uusd").with_balance(1_000000u64),
            },
        )
        .unwrap();
    assert_eq!(estimated_astro_out.u128(), 996006);
}

#[test]
fn update_owner() {
    let mut helper = Helper::new().unwrap();

    let new_owner = helper.app.api().addr_make("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.maker.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.maker.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    helper
        .app
        .execute_contract(helper.owner.clone(), helper.maker.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("invalid_addr"),
            helper.maker.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("invalid_addr"),
            helper.maker.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.maker.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Propose new owner
    helper
        .app
        .execute_contract(helper.owner.clone(), helper.maker.clone(), &msg, &[])
        .unwrap();

    // Claim ownership
    helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.maker.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap();

    let config: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.maker, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner.to_string(), new_owner)
}
