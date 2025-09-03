use astroport::asset::Asset;
use astroport::pair::ExecuteMsg;
use astroport_test::cw_multi_test::Executor;
use cosmwasm_std::{coin, Addr, Uint128};
use itertools::Itertools;

use astroport::maker::{AssetWithLimit, PoolRoute, MAX_SWAPS_DEPTH};
use astroport_maker::error::ContractError;

use crate::common::helper::{Helper, ASTRO_DENOM};

mod common;
#[test]
fn check_set_routes() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

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

    // Set wrong pool id
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: "ucoin".to_string(),
            asset_out: "uusd".to_string(),
            pool_addr: astro_pair.contract_addr.to_string(),
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidPoolAsset {
            pool_addr: astro_pool_id,
            denom: "ucoin".to_string()
        }
    );
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            denom_in: "ucoin".to_string(),
            denom_out: "rand".to_string(),
            pool_id: pool_1,
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidPoolAsset {
            pool_addr: pool_1,
            denom: "rand".to_string()
        }
    );

    // ucoin -> uusd -> astro
    helper
        .set_pool_routes(vec![
            PoolRoute {
                denom_in: "ucoin".to_string(),
                denom_out: "uusd".to_string(),
                pool_id: pool_1,
            },
            PoolRoute {
                denom_in: "uusd".to_string(),
                denom_out: ASTRO_DENOM.to_string(),
                pool_id: astro_pool_id,
            },
        ])
        .unwrap();

    let route = helper.query_route("ucoin");
    assert_eq!(
        route,
        vec![
            SwapRouteResponse {
                pool_id: pool_1,
                token_out_denom: "uusd".to_string(),
            },
            SwapRouteResponse {
                token_out_denom: ASTRO_DENOM.to_string(),
                pool_id: astro_pool_id,
            }
        ]
    );

    let (_, pool_2) = helper
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
            denom_in: "utest".to_string(),
            denom_out: "uusd".to_string(),
            pool_id: pool_2,
        }])
        .unwrap();

    let route = helper.query_route("utest");
    assert_eq!(
        route,
        vec![
            SwapRouteResponse {
                pool_id: pool_2,
                token_out_denom: "uusd".to_string(),
            },
            SwapRouteResponse {
                token_out_denom: ASTRO_DENOM.to_string(),
                pool_id: astro_pool_id,
            }
        ]
    );

    let (_, pool_3) = helper
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
            denom_in: "utest".to_string(),
            denom_out: "ucoin".to_string(),
            pool_id: pool_3,
        }])
        .unwrap();

    let route = helper.query_route("utest");
    assert_eq!(
        route,
        vec![
            SwapRouteResponse {
                pool_id: pool_3,
                token_out_denom: "ucoin".to_string(),
            },
            SwapRouteResponse {
                pool_id: pool_1,
                token_out_denom: "uusd".to_string(),
            },
            SwapRouteResponse {
                token_out_denom: ASTRO_DENOM.to_string(),
                pool_id: astro_pool_id,
            }
        ]
    );

    let (_, pool_4) = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "utest"),
            coin(1_000_000_000000, "uatomn"),
        ])
        .unwrap();

    // Trying to set route which doesn't lead to ASTRO
    //  utest -> uatomn
    //    x
    // ucoin -> uusd -> astro
    let err = helper
        .set_pool_routes(vec![PoolRoute {
            denom_in: "utest".to_string(),
            denom_out: "uatomn".to_string(),
            pool_id: pool_4,
        }])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::RouteNotFound {
            asset: "uatomn".to_string(),
        }
    );

    // Checking long swap path
    let mut routes = (0..=MAX_SWAPS_DEPTH)
        .into_iter()
        .tuple_windows()
        .map(|(i, j)| {
            let coin_a = format!("coin{i}");
            let coin_b = format!("coin{j}");
            let (_, pool_id) = helper
                .create_and_seed_pair([
                    coin(1_000_000_000000, &coin_a),
                    coin(1_000_000_000000, &coin_b),
                ])
                .unwrap();
            PoolRoute {
                denom_in: coin_a,
                denom_out: coin_b,
                pool_id,
            }
        })
        .collect_vec();

    let last_coin = format!("coin{MAX_SWAPS_DEPTH}");

    let (_, pool_id) = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, &last_coin),
            coin(1_000_000_000000, ASTRO_DENOM),
        ])
        .unwrap();

    routes.push(PoolRoute {
        denom_in: last_coin,
        denom_out: ASTRO_DENOM.to_string(),
        pool_id,
    });

    let err = helper.set_pool_routes(routes).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::FailedToBuildRoute {
            asset: "coin0".to_string(),
            route_taken: "coin0 -> coin1 -> coin2 -> coin3 -> coin4 -> coin5".to_string()
        }
    );
}

#[test]
fn test_collect() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let (_, astro_pool_id) = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, ASTRO_DENOM),
        ])
        .unwrap();

    helper
        .set_pool_routes(vec![PoolRoute {
            denom_in: "uusd".to_string(),
            denom_out: ASTRO_DENOM.to_string(),
            pool_id: astro_pool_id,
        }])
        .unwrap();

    // mock received fees
    let maker = helper.maker.clone();
    helper.give_me_money(&[Asset::native("uusd", 1_000000u64)], &maker);

    helper
        .collect(vec![AssetWithLimit {
            denom: "uusd".to_string(),
            amount: None,
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
    assert_eq!(astro_bal.amount.u128(), 998_048);

    let (_, pool_1) = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "coin_a"),
            coin(1_000_000_000000, "uusd"),
        ])
        .unwrap();
    let (_, pool_2) = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "coin_a"),
            coin(1_000_000_000000, "coin_b"),
        ])
        .unwrap();
    let (_, pool_3) = helper
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
                denom_in: "coin_a".to_string(),
                denom_out: "uusd".to_string(),
                pool_id: pool_1,
            },
            PoolRoute {
                denom_in: "coin_b".to_string(),
                denom_out: "coin_a".to_string(),
                pool_id: pool_2,
            },
            PoolRoute {
                denom_in: "coin_c".to_string(),
                denom_out: "uusd".to_string(),
                pool_id: pool_3,
            },
        ])
        .unwrap();

    helper.give_me_money(&[Asset::native("coin_a", 1_000000u64)], &maker);
    helper.give_me_money(&[Asset::native("coin_b", 1_000000u64)], &maker);
    helper.give_me_money(&[Asset::native("coin_c", 1_000000u64)], &maker);

    helper
        .collect(vec![
            AssetWithLimit {
                denom: "coin_a".to_string(),
                amount: None,
            },
            AssetWithLimit {
                denom: "coin_b".to_string(),
                amount: None,
            },
            AssetWithLimit {
                denom: "coin_c".to_string(),
                amount: None,
            },
        ])
        .unwrap();

    let coin_a_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_a")
        .unwrap();
    assert_eq!(coin_a_bal.amount.u128(), 649); // tiny fee left after swaps
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
    assert_eq!(astro_bal.amount.u128(), 3_981818);

    // Check collect with limit
    helper.give_me_money(&[Asset::native("coin_c", 1_000000u64)], &maker);
    helper
        .collect(vec![AssetWithLimit {
            denom: "coin_c".to_string(),
            amount: Some(500u128.into()),
        }])
        .unwrap();
    let coin_c_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "coin_c")
        .unwrap();
    assert_eq!(coin_c_bal.amount.u128(), 999_500);

    // Try to set limit higher than balance
    helper
        .collect(vec![AssetWithLimit {
            denom: "coin_c".to_string(),
            amount: Some(1_000_000u128.into()),
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
            &astroport::maker::QueryMsg::Routes {
                start_after: None,
                limit: Some(100),
            },
        )
        .unwrap();
    assert_eq!(
        routes,
        vec![
            PoolRoute {
                denom_in: "coin_a".to_string(),
                denom_out: "uusd".to_string(),
                pool_id: pool_1
            },
            PoolRoute {
                denom_in: "coin_b".to_string(),
                denom_out: "coin_a".to_string(),
                pool_id: pool_2
            },
            PoolRoute {
                denom_in: "coin_c".to_string(),
                denom_out: "uusd".to_string(),
                pool_id: pool_3
            },
            PoolRoute {
                denom_in: "uusd".to_string(),
                denom_out: "astro".to_string(),
                pool_id: astro_pool_id
            }
        ]
    );

    let estimated_astro_out: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.maker,
            &astroport::maker::QueryMsg::EstimateSwap {
                asset_in: coin(1_000000u128, "uusd"),
            },
        )
        .unwrap();
    assert_eq!(estimated_astro_out.u128(), 1000002);
}

#[test]
fn update_owner() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
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
        .execute_contract(
            Addr::unchecked(&helper.owner),
            helper.maker.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
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
            Addr::unchecked("invalid_addr"),
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
        .execute_contract(
            Addr::unchecked(&helper.owner),
            helper.maker.clone(),
            &msg,
            &[],
        )
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

    let config: astroport::maker::Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.maker, &astroport::maker::QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner.to_string(), new_owner)
}
