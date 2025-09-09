#![cfg(not(tarpaulin_include))]

use crate::common::helper::{f64_to_dec, Helper, MOCK_IBC_ESCROW};
use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::factory::PairType;
use astroport::maker::{
    AssetWithLimit, Config, DevFundConfig, ExecuteMsg, PoolRoute, QueryMsg, RouteStep, SeizeConfig,
    UpdateDevFundConfig, MAX_SWAPS_DEPTH,
};
use astroport::{factory, pair};
use astroport_maker::error::ContractError;
use astroport_test::cw_multi_test::Executor;
use cosmwasm_std::{coin, coins, Addr, Decimal, Uint128};
use cw20::BalanceResponse;
use itertools::Itertools;

mod common;

#[test]
fn test_set_routes() {
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, astro),
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
                asset_out: AssetInfo::native(astro),
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
                asset_out: AssetInfo::native(astro),
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
                asset_out: AssetInfo::native(astro),
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
                asset_out: AssetInfo::native(astro),
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
            coin(1_000_000_000000, astro),
        ])
        .unwrap();

    routes.push(PoolRoute {
        asset_in: AssetInfo::native(last_coin),
        asset_out: AssetInfo::native(astro),
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
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, astro),
        ])
        .unwrap();

    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("uusd"),
            asset_out: AssetInfo::native(astro),
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
        .query_balance(&helper.satellite, astro)
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
        .query_balance(&helper.satellite, astro)
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
                asset_out: AssetInfo::native(astro),
                pool_addr: astro_pair.contract_addr.to_string(),
            }
        ]
    );

    let asset_in = AssetInfo::native("uusd").with_balance(1_000000u64);
    let estimated_astro_out: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.maker,
            &QueryMsg::EstimateSwap {
                asset_in: asset_in.clone(),
            },
        )
        .unwrap();
    assert_eq!(estimated_astro_out.u128(), 996690);

    helper.give_me_money(&[asset_in], &maker);

    // Checking that duplicated routes as well as routes with empty assets don't cause any issues
    helper
        .collect(vec![
            AssetWithLimit {
                info: AssetInfo::native("uusd"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("coin_a"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("uusd"),
                limit: None,
            }, // <-- duplicated
            AssetWithLimit {
                info: AssetInfo::native("coin_b"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("uusd"),
                limit: None,
            }, // <-- duplicated
            AssetWithLimit {
                info: AssetInfo::native("coin_c"),
                limit: None,
            },
            AssetWithLimit {
                info: AssetInfo::native("uusd"),
                limit: None,
            }, // <-- duplicated
        ])
        .unwrap();
}

#[test]
fn test_collect_with_cw20() {
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();
    let owner = helper.owner.clone();
    let maker = helper.maker.clone();

    // Creating pairs and setting routes with the following scheme
    // xyz (cw20) -> uusdc -> astro

    let xyz_token = helper.init_cw20("XYZ").unwrap();

    let asset_infos = vec![
        AssetInfo::cw20_unchecked(&xyz_token),
        AssetInfo::native("uusdc"),
    ];
    let xyz_pair_info = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.factory.clone(),
            &factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Xyk {},
                asset_infos: asset_infos.clone(),
                init_params: None,
            },
            &[],
        )
        .map(|_| helper.query_pair_info(&asset_infos))
        .unwrap();

    helper.give_me_money(&[Asset::native("uusdc", 100_000000u128)], &owner);

    helper.mint_cw20(&xyz_token, &owner, 100_000000).unwrap();
    helper
        .set_allowance_cw20(&xyz_token, &owner, &xyz_pair_info.contract_addr, 100_000000)
        .unwrap();

    helper
        .app
        .execute_contract(
            owner.clone(),
            xyz_pair_info.contract_addr.clone(),
            &pair::ExecuteMsg::ProvideLiquidity {
                assets: vec![
                    Asset::native("uusdc", 100_000000u128),
                    Asset::cw20_unchecked(&xyz_token, 100_000000u128),
                ],
                slippage_tolerance: Some(f64_to_dec(0.5)),
                auto_stake: None,
                receiver: None,
                min_lp_to_receive: None,
            },
            &coins(100_000000, "uusdc"),
        )
        .unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusdc"),
            coin(1_000_000_000000, astro),
        ])
        .unwrap();

    helper
        .set_pool_routes(vec![
            PoolRoute {
                asset_in: AssetInfo::cw20_unchecked(&xyz_token),
                asset_out: AssetInfo::native("uusdc"),
                pool_addr: xyz_pair_info.contract_addr.to_string(),
            },
            PoolRoute {
                asset_in: AssetInfo::native("uusdc"),
                asset_out: AssetInfo::native(astro),
                pool_addr: astro_pair.contract_addr.to_string(),
            },
        ])
        .unwrap();

    // Collecting with an empty balance ends up in nothing to collect error
    let err = helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::cw20_unchecked(&xyz_token),
            limit: None,
        }])
        .unwrap_err();
    assert_eq!(ContractError::NothingToCollect {}, err.downcast().unwrap());

    // mock received XYZ fees
    helper.mint_cw20(&xyz_token, &maker, 1_000000).unwrap();

    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::cw20_unchecked(&xyz_token),
            limit: None,
        }])
        .unwrap();

    let uusd_bal = helper
        .app
        .wrap()
        .query_balance(&helper.maker, "uusdc")
        .unwrap();
    assert_eq!(uusd_bal.amount.u128(), 0);
    let xyz_bal: BalanceResponse = helper
        .app
        .wrap()
        .query_wasm_smart(
            &xyz_token,
            &cw20_base::msg::QueryMsg::Balance {
                address: maker.to_string(),
            },
        )
        .unwrap();
    assert_eq!(xyz_bal.balance.u128(), 0);

    let astro_bal = helper
        .app
        .wrap()
        .query_balance(&helper.satellite, astro)
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 985745);
}

#[test]
fn test_collect_outpost() {
    let astro = "ibc/astro";
    let mut helper = Helper::new(astro).unwrap();

    let astro_pair = helper
        .create_and_seed_pair([
            coin(1_000_000_000000, "uusd"),
            coin(1_000_000_000000, astro),
        ])
        .unwrap();

    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native("uusd"),
            asset_out: AssetInfo::native(astro),
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
        .query_balance(&helper.satellite, astro)
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 0);

    // Confirming that maker contract called TransferAstro endpoint and
    // sent all astro to a mocked ibc escrow account
    let astro_bal = helper
        .app
        .wrap()
        .query_balance(MOCK_IBC_ESCROW, astro)
        .unwrap();
    assert_eq!(astro_bal.amount.u128(), 997799);
}

#[test]
fn update_owner() {
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();

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

#[test]
fn test_seize() {
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();
    let owner = helper.owner.clone();
    let maker = helper.maker.clone();

    // try to seize an empty vector
    let err = helper.seize(&owner, vec![]).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: assets vector is empty"
    );

    let seize_assets = vec![AssetWithLimit {
        info: AssetInfo::native("uusdc"),
        limit: None,
    }];

    // Try to seize before config is set
    let err = helper.seize(&owner, seize_assets.clone()).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: No seizable assets found"
    );

    // Unauthorized check
    let rand_user = helper.app.api().addr_make("rand_user");
    let err = helper
        .app
        .execute_contract(
            rand_user,
            maker.clone(),
            &ExecuteMsg::UpdateSeizeConfig {
                receiver: None,
                seizable_assets: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let receiver = helper.app.api().addr_make("receiver");

    let usdc = "uusdc";
    let luna = "uluna";

    // Set valid config
    helper
        .app
        .execute_contract(
            owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateSeizeConfig {
                receiver: Some(receiver.to_string()),
                seizable_assets: vec![AssetInfo::native(usdc), AssetInfo::native(luna)],
            },
            &[],
        )
        .unwrap();

    // Assert that the config is set
    assert_eq!(
        helper.query_seize_config().unwrap(),
        SeizeConfig {
            receiver: receiver.clone(),
            seizable_assets: vec![AssetInfo::native(usdc), AssetInfo::native(luna)]
        }
    );

    // Try to seize non-seizable asset
    let err = helper
        .seize(
            &owner,
            vec![AssetWithLimit {
                info: AssetInfo::native("utest"),
                limit: None,
            }],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Input vector contains assets that are not seizable"
    );

    // Try to seize asset with empty balance
    // This does nothing and doesn't throw an error
    helper
        .seize(
            &owner,
            vec![AssetWithLimit {
                info: AssetInfo::native(luna),
                limit: None,
            }],
        )
        .unwrap();

    helper.give_me_money(
        &[
            Asset::native(usdc, 1000_000000u128),
            Asset::native(luna, 3000_000000u128),
        ],
        &maker,
    );

    // Seize 100 USDC
    helper
        .seize(
            &owner,
            vec![AssetWithLimit {
                info: AssetInfo::native(usdc),
                limit: Some(100_000000u128.into()),
            }],
        )
        .unwrap();

    // Check balances
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, usdc)
            .unwrap()
            .amount
            .u128(),
        900_000000
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&receiver, usdc)
            .unwrap()
            .amount
            .u128(),
        100_000000
    );

    // Seize all
    helper
        .seize(
            &owner,
            vec![
                AssetWithLimit {
                    info: AssetInfo::native(usdc),
                    // seizing more than available doesn't throw an error
                    limit: Some(10000_000000u128.into()),
                },
                AssetWithLimit {
                    info: AssetInfo::native(luna),
                    limit: Some(3000_000000u128.into()),
                },
            ],
        )
        .unwrap();

    // Check balances
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, luna)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&receiver, usdc)
            .unwrap()
            .amount
            .u128(),
        1000_000000
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&receiver, luna)
            .unwrap()
            .amount
            .u128(),
        3000_000000
    );
}

#[test]
fn test_dev_fund_fee() {
    let astro = "astro";
    let mut helper = Helper::new(astro).unwrap();
    let owner = helper.owner.clone();
    let maker = helper.maker.clone();
    let fee_collector = helper.query_config().unwrap().collector;
    let usdc = "uusdc";

    let mut dev_fund_conf = DevFundConfig {
        address: "".to_string(),
        share: Default::default(),
        asset_info: AssetInfo::native(usdc),
        pool_addr: Addr::unchecked(""),
    };

    let err = helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Invalid input");

    dev_fund_conf.address = helper.app.api().addr_make("devs").to_string();

    let err = helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Dev fund share must be > 0 and <= 1"
    );

    dev_fund_conf.share = Decimal::percent(50);

    let err = helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Querier contract error: Generic error: Pair not found"
    );

    // Create a random pool and try to set it in the dev fund config
    let faulty_pair_info = helper
        .create_and_seed_pair([coin(100_000_000000, "foo"), coin(100_000_000000, astro)])
        .unwrap();
    dev_fund_conf.pool_addr = faulty_pair_info.contract_addr.clone();

    let err = helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidPoolAsset {
            pool_addr: faulty_pair_info.contract_addr.to_string(),
            asset: usdc.to_string()
        },
        err.downcast().unwrap()
    );

    // Create ASTRO<>USDC pool
    let pair_info = helper
        .create_and_seed_pair([coin(100_000_000000, usdc), coin(100_000_000000, astro)])
        .unwrap();
    dev_fund_conf.pool_addr = pair_info.contract_addr.clone();

    let err = helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap_err();
    assert_eq!(
        ContractError::RouteNotFound {
            asset: usdc.to_string()
        },
        err.downcast().unwrap()
    );

    // Set usdc <> astro route
    helper
        .set_pool_routes(vec![PoolRoute {
            asset_in: AssetInfo::native(usdc),
            asset_out: AssetInfo::native(astro),
            pool_addr: pair_info.contract_addr.to_string(),
        }])
        .unwrap();

    helper
        .set_dev_fund_config(
            &owner,
            UpdateDevFundConfig {
                set: Some(dev_fund_conf.clone()),
            },
        )
        .unwrap();

    // Emulate usdc income to the Maker contract
    helper.give_me_money(&[Asset::native(usdc, 1000_000000u128)], &maker);

    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::native(usdc),
            limit: None,
        }])
        .unwrap();

    // Check balances
    // ASTRO
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, astro)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&fee_collector, astro)
            .unwrap()
            .amount
            .u128(),
        493_960341
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&dev_fund_conf.address, astro)
            .unwrap()
            .amount
            .u128(),
        0
    );
    // USDC
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&dev_fund_conf.address, usdc)
            .unwrap()
            .amount
            .u128(),
        500_273461
    );

    // Disable dev funds
    helper
        .set_dev_fund_config(&owner, UpdateDevFundConfig { set: None })
        .unwrap();

    // Emulate usdc income to the Maker contract
    helper.give_me_money(&[Asset::native(usdc, 1000_000000u128)], &maker);

    helper
        .collect(vec![AssetWithLimit {
            info: AssetInfo::native(usdc),
            limit: None,
        }])
        .unwrap();

    // Check balances
    // ASTRO
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, astro)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&fee_collector, astro)
            .unwrap()
            .amount
            .u128(),
        1472_161157
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&dev_fund_conf.address, astro)
            .unwrap()
            .amount
            .u128(),
        0
    );
    // USDC
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&maker, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        helper
            .app
            .wrap()
            .query_balance(&dev_fund_conf.address, usdc)
            .unwrap()
            .amount
            .u128(),
        500_273461
    );
}
