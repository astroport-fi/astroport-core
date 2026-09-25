#![cfg(not(tarpaulin_include))]

use cosmwasm_std::{coins, from_json, to_json_binary, Addr, Decimal, Empty, StdError, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};

use astroport::asset::{native_asset_info, token_asset_info, AssetInfo};
use astroport::factory::PairType;
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::router::{
    ExecuteMsg, InstantiateMsg, QueryMsg, SimulateSwapOperationsResponse, SwapOperation,
    SwapResponseData,
};
use astroport_router::error::ContractError;
use astroport_test::convert::f64_to_dec;
use astroport_test::cw_multi_test::{AppBuilder, Contract, ContractWrapper, Executor};
use astroport_test::modules::stargate::{MockStargate, StargateApp as App};

use crate::factory_helper::{instantiate_token, mint, mint_native, FactoryHelper};

mod factory_helper;

fn router_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_router::contract::execute,
            astroport_router::contract::instantiate,
            astroport_router::contract::query,
        )
        .with_reply_empty(astroport_router::contract::reply),
    )
}

fn mock_app() -> App {
    AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {})
}

#[test]
fn router_does_not_enforce_spread_assertion() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token_x = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOX", None);
    let token_y = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOY", None);
    let token_z = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOZ", None);

    for (a, b, typ, liq) in [
        (&token_x, &token_y, PairType::Xyk {}, 100_000_000000),
        (&token_y, &token_z, PairType::Stable {}, 1_000_000_000000),
    ] {
        let pair = helper
            .create_pair(
                &mut app,
                &owner,
                typ,
                [token_asset_info(a.clone()), token_asset_info(b.clone())],
                None,
            )
            .unwrap();
        mint(&mut app, &owner, a, liq, &pair).unwrap();
        mint(&mut app, &owner, b, liq, &pair).unwrap();
    }

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    // Triggering swap with a huge spread fees
    mint(&mut app, &owner, &token_x, 50_000_000000, &owner).unwrap();
    let resp = app
        .execute_contract(
            owner.clone(),
            token_x.clone(),
            &Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: 50_000_000000u128.into(),
                msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                    operations: vec![
                        SwapOperation::AstroSwap {
                            offer_asset_info: token_asset_info(token_x.clone()),
                            ask_asset_info: token_asset_info(token_y.clone()),
                        },
                        SwapOperation::AstroSwap {
                            offer_asset_info: token_asset_info(token_y.clone()),
                            ask_asset_info: token_asset_info(token_z.clone()),
                        },
                    ],
                    minimum_receive: Uint128::new(1),
                    to: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();

    // We can't set data in response if the first message dispatched from cw20 contract
    assert!(
        resp.data.is_none(),
        "Unexpected data set after cw20 send hook"
    );

    // Single hops don't enforce the pool's spread check either; minimum_receive does the job
    let single_hop = |minimum_receive: u128| {
        to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
            operations: vec![SwapOperation::AstroSwap {
                offer_asset_info: token_asset_info(token_x.clone()),
                ask_asset_info: token_asset_info(token_y.clone()),
            }],
            minimum_receive: minimum_receive.into(),
            to: None,
        })
        .unwrap()
    };

    // a minimum above what the pool returns fails the swap
    mint(&mut app, &owner, &token_x, 50_000_000000, &owner).unwrap();
    let err = app
        .execute_contract(
            owner.clone(),
            token_x.clone(),
            &Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: 50_000_000000u128.into(),
                msg: single_hop(50_000_000000),
            },
            &[],
        )
        .unwrap_err();
    assert!(
        matches!(
            err.downcast::<ContractError>().unwrap(),
            ContractError::AssertionMinimumReceive { .. }
        ),
        "expected the minimum receive to fail the swap"
    );

    // the same huge-spread swap goes through when it meets the minimum
    app.execute_contract(
        owner.clone(),
        token_x.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: 50_000_000000u128.into(),
            msg: single_hop(1),
        },
        &[],
    )
    .unwrap();
}

#[test]
fn route_through_pairs_with_natives() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let denom_x = "denom_x";
    let denom_y = "denom_y";
    let denom_z = "denom_z";

    for (a, b, typ, liq) in [
        (&denom_x, &denom_y, PairType::Xyk {}, 100_000_000000),
        (&denom_y, &denom_z, PairType::Stable {}, 1_000_000_000000),
    ] {
        let pair = helper
            .create_pair(
                &mut app,
                &owner,
                typ,
                [
                    native_asset_info(a.to_string()),
                    native_asset_info(b.to_string()),
                ],
                None,
            )
            .unwrap();
        mint_native(&mut app, a, liq, &pair).unwrap();
        mint_native(&mut app, b, liq, &pair).unwrap();
    }

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    // Sanity checks

    let err = app
        .execute_contract(
            owner.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperation {
                operation: SwapOperation::AstroSwap {
                    offer_asset_info: native_asset_info(denom_x.to_string()),
                    ask_asset_info: native_asset_info(denom_y.to_string()),
                },
                to: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );
    let err = app
        .execute_contract(
            owner.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![SwapOperation::AstroSwap {
                    offer_asset_info: native_asset_info(denom_x.to_string()),
                    ask_asset_info: native_asset_info(denom_y.to_string()),
                }],
                to: None,
                // the pools' spread checks are off, so a route must set a real minimum
                minimum_receive: Uint128::zero(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::MinimumReceiveRequired {}
    );

    let err = app
        .execute_contract(
            owner.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![SwapOperation::AstroSwap {
                    offer_asset_info: native_asset_info(denom_x.to_string()),
                    ask_asset_info: native_asset_info(denom_x.to_string()),
                }],
                to: None,
                minimum_receive: Uint128::new(1),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DoublingAssetsPath {
            offer_asset: denom_x.to_string(),
            ask_asset: denom_x.to_string()
        }
    );

    // End sanity checks

    mint_native(&mut app, &denom_x, 50_000_000000, &owner).unwrap();
    let resp = app
        .execute_contract(
            owner.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![
                    SwapOperation::AstroSwap {
                        offer_asset_info: native_asset_info(denom_x.to_string()),
                        ask_asset_info: native_asset_info(denom_y.to_string()),
                    },
                    SwapOperation::AstroSwap {
                        offer_asset_info: native_asset_info(denom_y.to_string()),
                        ask_asset_info: native_asset_info(denom_z.to_string()),
                    },
                ],
                minimum_receive: Uint128::new(1),
                to: None,
            },
            &coins(50_000_000000, denom_x),
        )
        .unwrap();

    let resp_data: SwapResponseData = from_json(&resp.data.unwrap()).unwrap();

    assert_eq!(resp_data.return_amount.u128(), 32_258_064515);

    mint_native(&mut app, &denom_x, 50_000_000000, &owner).unwrap();
    let err = app
        .execute_contract(
            owner.clone(),
            router,
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![
                    SwapOperation::AstroSwap {
                        offer_asset_info: native_asset_info(denom_x.to_string()),
                        ask_asset_info: native_asset_info(denom_y.to_string()),
                    },
                    SwapOperation::AstroSwap {
                        offer_asset_info: native_asset_info(denom_y.to_string()),
                        ask_asset_info: native_asset_info(denom_z.to_string()),
                    },
                ],
                minimum_receive: 50_000_000000u128.into(), // <--- enforcing minimum receive with 1:1 rate (which practically impossible,
                to: None,
            },
            &coins(50_000_000000, denom_x),
        )
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AssertionMinimumReceive {
            receive: 50_000_000000u128.into(),
            amount: 15_360_983102u128.into()
        }
    );
}

#[test]
fn test_swap_route() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);
    let astro = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "astro", None);
    let inj = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "inj", None);
    let atom = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "atom", None);
    let osmo = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "osmo", None);

    for (a, b, typ, liq) in [
        (&astro, &inj, PairType::Xyk {}, 100_000_000000),
        (&inj, &osmo, PairType::Xyk {}, 100_000_000000),
        (&atom, &osmo, PairType::Xyk {}, 100_000_000000),
    ] {
        let pair = helper
            .create_pair(
                &mut app,
                &owner,
                typ,
                [token_asset_info(a.clone()), token_asset_info(b.clone())],
                None,
            )
            .unwrap();
        mint(&mut app, &owner, a, liq, &pair).unwrap();
        mint(&mut app, &owner, b, liq, &pair).unwrap();
    }
    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    let swap_amount = Uint128::new(10_000_000);

    // Try to swap with a bad batch of path
    // route: astro -> inj, atom -> osmo
    let swap_operations = vec![
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: astro.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: inj.clone(),
            },
        },
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: atom.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: osmo.clone(),
            },
        },
    ];

    let err = app
        .wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            router.clone(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount: swap_amount,
                operations: swap_operations.clone(),
            },
        )
        .unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err(
            "Querier contract error: The next offer asset must be \
    the same as the previous ask asset; contract3 --> contract4 --> contract5"
        )
    );

    // swap astro for osmo
    // route: astro -> inj, inj -> osmo, osmo -> atom, atom -> osmo
    let swap_operations = vec![
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: astro.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: inj.clone(),
            },
        },
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: inj.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: osmo.clone(),
            },
        },
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: osmo.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: atom.clone(),
            },
        },
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: atom.clone(),
            },
            ask_asset_info: AssetInfo::Token {
                contract_addr: osmo.clone(),
            },
        },
    ];

    // the simulation succeeds
    let simulate_res: SimulateSwapOperationsResponse = app
        .wrap()
        .query_wasm_smart(
            router.clone(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount: swap_amount,
                operations: swap_operations.clone(),
            },
        )
        .unwrap();

    assert_eq!(simulate_res.amount, Uint128::new(9996000));
    println!(
        "0. User simulate swap, expected return amount: {:?}",
        simulate_res.amount
    );

    let user = Addr::unchecked("user");
    mint(&mut app, &owner, &astro, swap_amount.u128(), &user).unwrap();

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, swap_amount);

    // swap
    app.execute_contract(
        user.clone(),
        astro.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: swap_amount,
            msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                operations: swap_operations.clone(),
                minimum_receive: Uint128::new(1),
                to: None,
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    let attacker = Addr::unchecked("attacker");
    let donated_atom: u128 = 1;

    mint(&mut app, &owner, &atom, donated_atom, &attacker).unwrap();

    // attacker donates little amount to router contract
    app.execute_contract(
        attacker.clone(),
        atom.clone(),
        &Cw20ExecuteMsg::Transfer {
            recipient: router.to_string(),
            amount: Uint128::new(donated_atom),
        },
        &[],
    )
    .unwrap();

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::new(1));

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::new(9997999));

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    // mint more astro to user
    mint(&mut app, &owner, &astro, swap_amount.u128(), &user).unwrap();

    // victim tx gets executed with a token minimum of 1, i.e. effectively no protection
    app.execute_contract(
        user.clone(),
        astro.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: swap_amount,
            msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                operations: swap_operations.clone(),
                minimum_receive: Uint128::new(1),
                to: None,
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // Query victim balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::new(19992001));

    // Query router contract balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    println!("OSMO router balance: {:?}", balance_res.balance);

    // attacker try back-runs the tx and withdraw nothing
    let err = app
        .execute_contract(
            attacker.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: osmo.clone(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: atom.clone(),
                    },
                }],
                minimum_receive: Uint128::new(9_997_000),
                to: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Swap amount must not be zero"
    );

    // Query attacker balance and calculate profit
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: attacker.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: attacker.to_string(),
            },
        )
        .unwrap();

    println!("ATOM attacker balance: {:?}", balance_res.balance);
    println!("Donated ATOM: {:?}", donated_atom);

    let profit = balance_res
        .balance
        .saturating_sub(Uint128::new(donated_atom));
    println!("Attacker's profit: {:?}", profit);

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: attacker.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    // double check router contract have no funds left
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    /* -------------------------------------------------------------------------------------------
    2. lets try attack with a real minimum_receive.
    -------------------------------------------------------------------------------------------*/
    println!("\n2. Assume user provides a real `minimum_receive`");

    mint(&mut app, &owner, &astro, swap_amount.u128(), &user).unwrap();

    // query balance
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, swap_amount);

    // attacker2 front-run tx
    let attacker2 = Addr::unchecked("attacker2");

    // assume the market is bad and user wants to get as much as they can
    let donated_atom = Uint128::new(9_000_000);

    // attacker2 donate funds
    mint(&mut app, &owner, &atom, donated_atom.u128(), &attacker2).unwrap();

    app.execute_contract(
        attacker2.clone(),
        atom.clone(),
        &Cw20ExecuteMsg::Transfer {
            recipient: router.to_string(),
            amount: donated_atom,
        },
        &[],
    )
    .unwrap();

    // victim tx gets executed
    app.execute_contract(
        user.clone(),
        astro.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: swap_amount,
            msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                operations: swap_operations.clone(),
                minimum_receive: donated_atom,
                to: None,
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // query router contract
    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    println!("ASTRO router balance: {:?}", balance_res.balance);

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    println!("ATOM router balance: {:?}", balance_res.balance);

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            osmo.clone(),
            &Cw20QueryMsg::Balance {
                address: router.to_string(),
            },
        )
        .unwrap();
    println!("OSMO router balance: {:?}", balance_res.balance);

    // attacker back-runs tx to withdraw funds
    let err = app
        .execute_contract(
            attacker2.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: vec![SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: osmo.clone(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: atom.clone(),
                    },
                }],
                minimum_receive: Uint128::new(1),
                to: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Swap amount must not be zero"
    );

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            astro.clone(),
            &Cw20QueryMsg::Balance {
                address: attacker2.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_res.balance, Uint128::zero());

    let balance_res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            atom.clone(),
            &Cw20QueryMsg::Balance {
                address: attacker2.to_string(),
            },
        )
        .unwrap();

    println!("ATOM attacker2 balance: {:?}", balance_res.balance);
    println!("Donated ATOM: {:?}", donated_atom);

    let profit = balance_res.balance.saturating_sub(donated_atom);
    println!("Attacker2's profit: {:?}", profit);
}

#[test]
fn test_reverse_simulation() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let denom_x = "denom_x";
    let denom_y = "denom_y";
    let denom_z = "denom_z";

    for (a, b, liq) in [
        (&denom_x, &denom_y, 100_000_000000),
        (&denom_x, &denom_z, 100_000_000000),
        (&denom_y, &denom_z, 100_000_000000),
    ] {
        let pair = helper
            .create_pair(
                &mut app,
                &owner,
                PairType::Custom("concentrated".to_string()),
                [
                    native_asset_info(a.to_string()),
                    native_asset_info(b.to_string()),
                ],
                Some(
                    to_json_binary(&ConcentratedPoolParams {
                        amp: f64_to_dec(10f64),
                        gamma: f64_to_dec(0.000145),
                        mid_fee: f64_to_dec(0.0026),
                        out_fee: f64_to_dec(0.0045),
                        fee_gamma: f64_to_dec(0.00023),
                        repeg_profit_threshold: f64_to_dec(0.000002),
                        min_price_scale_delta: f64_to_dec(0.000146),
                        price_scale: Decimal::from_ratio(2u8, 1u8),
                        ma_half_time: 600,
                        track_asset_balances: None,
                        fee_share: None,
                        allowed_xcp_profit_drop: None,
                        xcp_profit_losses_threshold: None,
                    })
                    .unwrap(),
                ),
            )
            .unwrap();
        mint_native(&mut app, a, liq, &pair).unwrap();
        mint_native(&mut app, b, liq / 2, &pair).unwrap();
    }

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    let operations = vec![
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::native(denom_x),
            ask_asset_info: AssetInfo::native(denom_y),
        },
        SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::native(denom_y),
            ask_asset_info: AssetInfo::native(denom_z),
        },
    ];

    let ask_amount = Uint128::new(1_000_000000);
    let offer_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(
            router.clone(),
            &QueryMsg::ReverseSimulateSwapOperations {
                ask_amount,
                operations: operations.clone(),
            },
        )
        .unwrap();

    let return_amount = app
        .wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            router.clone(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount,
                operations,
            },
        )
        .unwrap()
        .amount;

    // ensure return amount is greater or equal to the requested amount
    assert!(
        return_amount >= ask_amount,
        "Return amount is less than ask amount: {return_amount} >= {ask_amount}"
    );
}

fn transmuter_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_transmuter::contract::execute,
            astroport_pair_transmuter::contract::instantiate,
            astroport_pair_transmuter::queries::query,
        )
        .with_reply_empty(astroport_pair_transmuter::contract::reply),
    )
}

/// A transmuter instantiated directly (not through the factory) can't be reached with
/// AstroSwap, but PoolSwap can route through it, including as a later hop of a multi-hop swap.
#[test]
fn route_through_standalone_transmuter() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let luna = "uluna";
    let usdc_n = "usdc_noble";
    let usdc_inj = "usdc_inj";

    // factory LUNA/USDC.n pool
    let luna_usdc = helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [
                native_asset_info(luna.to_string()),
                native_asset_info(usdc_n.to_string()),
            ],
            None,
        )
        .unwrap();
    mint_native(&mut app, luna, 1_000_000_000000, &luna_usdc).unwrap();
    mint_native(&mut app, usdc_n, 50_000_000000, &luna_usdc).unwrap();

    // standalone USDC.n/USDC.inj transmuter, like the one on Terra
    app.execute_contract(
        owner.clone(),
        helper.coin_registry.clone(),
        &astroport::native_coin_registry::ExecuteMsg::Add {
            native_coins: vec![(usdc_inj.to_string(), 6)],
        },
        &[],
    )
    .unwrap();
    let transmuter_code = app.store_code(transmuter_contract());
    let transmuter = app
        .instantiate_contract(
            transmuter_code,
            owner.clone(),
            &astroport::pair::InstantiateMsg {
                pair_type: PairType::Custom("transmuter".to_string()),
                asset_infos: vec![
                    native_asset_info(usdc_n.to_string()),
                    native_asset_info(usdc_inj.to_string()),
                ],
                token_code_id: helper.cw20_token_code_id,
                factory_addr: helper.factory.to_string(),
                init_params: None,
            },
            &[],
            "usdc transmuter",
            None,
        )
        .unwrap();
    mint_native(&mut app, usdc_inj, 10_000_000000, &transmuter).unwrap();

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    // the factory doesn't know the transmuter, so AstroSwap can't find it
    let astro_swap_ops = vec![SwapOperation::AstroSwap {
        offer_asset_info: native_asset_info(usdc_n.to_string()),
        ask_asset_info: native_asset_info(usdc_inj.to_string()),
    }];
    app.wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: 1_000000u128.into(),
                operations: astro_swap_ops,
            },
        )
        .unwrap_err();

    let operations = vec![
        SwapOperation::AstroSwap {
            offer_asset_info: native_asset_info(luna.to_string()),
            ask_asset_info: native_asset_info(usdc_n.to_string()),
        },
        SwapOperation::PoolSwap {
            pool_addr: transmuter.to_string(),
            offer_asset_info: native_asset_info(usdc_n.to_string()),
            ask_asset_info: native_asset_info(usdc_inj.to_string()),
        },
    ];

    let offer_amount = 100_000000u128;
    let first_hop: SimulateSwapOperationsResponse = app
        .wrap()
        .query_wasm_smart(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: offer_amount.into(),
                operations: operations[..1].to_vec(),
            },
        )
        .unwrap();
    let simulated: SimulateSwapOperationsResponse = app
        .wrap()
        .query_wasm_smart(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: offer_amount.into(),
                operations: operations.clone(),
            },
        )
        .unwrap();
    // the transmuter hop is 1:1, so the route returns exactly what the first hop does
    assert_eq!(simulated.amount, first_hop.amount);
    assert!(!simulated.amount.is_zero());

    let reverse: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &router,
            &QueryMsg::ReverseSimulateSwapOperations {
                ask_amount: simulated.amount,
                operations: operations.clone(),
            },
        )
        .unwrap();
    // xyk reverse simulation rounds up, so it can need a unit or so more than the offer
    assert!(
        reverse.u128().abs_diff(offer_amount) <= 2,
        "reverse simulation {reverse} too far from {offer_amount}"
    );

    // a minimum receive above the route's output fails the whole swap
    mint_native(&mut app, luna, offer_amount, &user).unwrap();
    let err = app
        .execute_contract(
            user.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: operations.clone(),
                minimum_receive: simulated.amount + Uint128::one(),
                to: None,
            },
            &coins(offer_amount, luna),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AssertionMinimumReceive {
            receive: simulated.amount + Uint128::one(),
            amount: simulated.amount,
        }
    );

    let resp = app
        .execute_contract(
            user.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: operations.clone(),
                minimum_receive: simulated.amount,
                to: None,
            },
            &coins(offer_amount, luna),
        )
        .unwrap();
    let resp_data: SwapResponseData = from_json(resp.data.unwrap()).unwrap();
    assert_eq!(resp_data.return_amount, simulated.amount);

    let user_usdc_inj = app.wrap().query_balance(&user, usdc_inj).unwrap().amount;
    assert_eq!(user_usdc_inj, simulated.amount);
    // nothing is left behind in the router, funds or route data
    assert!(app.wrap().query_all_balances(&router).unwrap().is_empty());
    assert!(app
        .wrap()
        .query_wasm_raw(&router, b"reply_data".as_slice())
        .unwrap()
        .is_none());
}

#[test]
fn pool_swap_rejects_pools_without_the_assets() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let denom_x = "denom_x";
    let denom_y = "denom_y";
    let denom_z = "denom_z";

    let pair = helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [
                native_asset_info(denom_x.to_string()),
                native_asset_info(denom_y.to_string()),
            ],
            None,
        )
        .unwrap();
    mint_native(&mut app, denom_x, 1_000_000000, &pair).unwrap();
    mint_native(&mut app, denom_y, 1_000_000000, &pair).unwrap();

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    // the pool holds X/Y, not Z
    let wrong_asset = vec![SwapOperation::PoolSwap {
        pool_addr: pair.to_string(),
        offer_asset_info: native_asset_info(denom_x.to_string()),
        ask_asset_info: native_asset_info(denom_z.to_string()),
    }];
    let expected = ContractError::PoolAssetsMismatch {
        pool: pair.to_string(),
        offer_asset: denom_x.to_string(),
        ask_asset: denom_z.to_string(),
    };

    let err = app
        .wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: 1_000000u128.into(),
                operations: wrong_asset.clone(),
            },
        )
        .unwrap_err();
    assert!(
        err.to_string().contains(&expected.to_string()),
        "unexpected error: {err}"
    );

    mint_native(&mut app, denom_x, 1_000000, &owner).unwrap();
    let err = app
        .execute_contract(
            owner.clone(),
            router.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations: wrong_asset,
                minimum_receive: Uint128::new(1),
                to: None,
            },
            &coins(1_000000, denom_x),
        )
        .unwrap_err();
    assert_eq!(err.downcast::<ContractError>().unwrap(), expected);

    // an address that isn't a pool at all
    let err = app
        .wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: 1_000000u128.into(),
                operations: vec![SwapOperation::PoolSwap {
                    pool_addr: "not_a_pool".to_string(),
                    offer_asset_info: native_asset_info(denom_x.to_string()),
                    ask_asset_info: native_asset_info(denom_y.to_string()),
                }],
            },
        )
        .unwrap_err();
    assert!(!err.to_string().is_empty());

    // PoolSwap and AstroSwap on the same factory pool give the same quote
    let quote = |op: SwapOperation| -> Uint128 {
        app.wrap()
            .query_wasm_smart::<SimulateSwapOperationsResponse>(
                &router,
                &QueryMsg::SimulateSwapOperations {
                    offer_amount: 1_000000u128.into(),
                    operations: vec![op],
                },
            )
            .unwrap()
            .amount
    };
    assert_eq!(
        quote(SwapOperation::PoolSwap {
            pool_addr: pair.to_string(),
            offer_asset_info: native_asset_info(denom_x.to_string()),
            ask_asset_info: native_asset_info(denom_y.to_string()),
        }),
        quote(SwapOperation::AstroSwap {
            offer_asset_info: native_asset_info(denom_x.to_string()),
            ask_asset_info: native_asset_info(denom_y.to_string()),
        })
    );
}

/// A contract whose `pair {}` reports a different address, e.g. a proxy in front of a pool.
fn misreporting_pool_contract() -> Box<dyn Contract<Empty>> {
    use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

    fn instantiate(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn execute(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn query(_: Deps, _: Env, _: astroport::pair::QueryMsg) -> StdResult<Binary> {
        to_json_binary(&astroport::asset::PairInfo {
            asset_infos: vec![
                native_asset_info("denom_x".to_string()),
                native_asset_info("denom_y".to_string()),
            ],
            contract_addr: Addr::unchecked("some_other_pool"),
            liquidity_token: String::new(),
            pair_type: PairType::Xyk {},
        })
    }

    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

#[test]
fn pool_swap_rejects_pools_reporting_another_address() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let helper = FactoryHelper::init(&mut app, &owner);

    let misreporting_code = app.store_code(misreporting_pool_contract());
    let misreporting = app
        .instantiate_contract(
            misreporting_code,
            owner.clone(),
            &Empty {},
            &[],
            "misreporting pool",
            None,
        )
        .unwrap();

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    let err = app
        .wrap()
        .query_wasm_smart::<SimulateSwapOperationsResponse>(
            &router,
            &QueryMsg::SimulateSwapOperations {
                offer_amount: 1_000000u128.into(),
                operations: vec![SwapOperation::PoolSwap {
                    pool_addr: misreporting.to_string(),
                    offer_asset_info: native_asset_info("denom_x".to_string()),
                    ask_asset_info: native_asset_info("denom_y".to_string()),
                }],
            },
        )
        .unwrap_err();
    let expected = ContractError::PoolAddressMismatch {
        pool: misreporting.to_string(),
        reported: "some_other_pool".to_string(),
    };
    assert!(
        err.to_string().contains(&expected.to_string()),
        "unexpected error: {err}"
    );
}
