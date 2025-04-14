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
                    minimum_receive: None,
                    to: None,
                    max_spread: None,
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

    // However, single hop will still enforce spread assertion
    mint(&mut app, &owner, &token_x, 50_000_000000, &owner).unwrap();
    let err = app
        .execute_contract(
            owner.clone(),
            token_x.clone(),
            &Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: 50_000_000000u128.into(),
                msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                    operations: vec![SwapOperation::AstroSwap {
                        offer_asset_info: token_asset_info(token_x.clone()),
                        ask_asset_info: token_asset_info(token_y.clone()),
                    }],
                    minimum_receive: None,
                    to: None,
                    max_spread: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        astroport_pair::error::ContractError::MaxSpreadAssertion {},
        err.downcast().unwrap()
    )
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
                max_spread: None,
                single: false,
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
                operations: vec![SwapOperation::NativeSwap {
                    offer_denom: denom_x.to_string(),
                    ask_denom: denom_y.to_string(),
                }],
                to: None,
                max_spread: None,
                minimum_receive: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NativeSwapNotSupported {}
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
                max_spread: None,
                minimum_receive: None,
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
                minimum_receive: None,
                to: None,
                max_spread: None,
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
                minimum_receive: Some(50_000_000000u128.into()), // <--- enforcing minimum receive with 1:1 rate (which practically impossible)
                to: None,
                max_spread: None,
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
                minimum_receive: None,
                to: None,
                max_spread: None,
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

    // victim tx gets executed. Assume user provide `minimum_receive` as `None`"
    app.execute_contract(
        user.clone(),
        astro.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: swap_amount,
            msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                operations: swap_operations.clone(),
                minimum_receive: None,
                to: None,
                max_spread: None,
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
                minimum_receive: Some(Uint128::new(9_997_000)),
                to: None,
                max_spread: None,
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
    2. lets try attack with minimum_receive as Some(_).
    -------------------------------------------------------------------------------------------*/
    println!("\n2. Assume user provide `minimum_receive` as `Some(_)`");

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
                minimum_receive: Some(donated_atom),
                to: None,
                max_spread: None,
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
                minimum_receive: None,
                to: None,
                max_spread: None,
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
