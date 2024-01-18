#![cfg(not(tarpaulin_include))]

use cosmwasm_std::Addr;

use astroport::staking::{Config, QueryMsg, TrackerData};

use crate::common::helper::{Helper, ASTRO_DENOM};
use crate::common::neutron_ext::TOKEN_FACTORY_MODULE;

mod common;

const ALICE: &str = "alice";
const BOB: &str = "bob";
const CAROL: &str = "carol";
const ATTACKER: &str = "attacker";
const VICTIM: &str = "victim";

#[test]
fn test_instantiate_tokenfactory() {
    let owner = Addr::unchecked("owner");

    let helper = Helper::new(&owner).unwrap();

    let response: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        response,
        Config {
            astro_denom: ASTRO_DENOM.to_string(),
            xastro_denom: format!("factory/{}/xASTRO", &helper.staking)
        }
    );

    let response: TrackerData = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::TrackerConfig {})
        .unwrap();
    assert_eq!(
        response,
        TrackerData {
            code_id: 2,
            admin: owner.to_string(),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
            tracker_addr: "contract1".to_string(),
        }
    );
}
//
// #[test]
// fn check_deflate_liquidity() {
//     let mut router = mock_app();
//
//     let owner = Addr::unchecked("owner");
//
//     let (astro_token_instance, staking_instance, _) =
//         instantiate_contracts(&mut router, owner.clone());
//
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         ATTACKER,
//     );
//
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         VICTIM,
//     );
//
//     let attacker_address = Addr::unchecked(ATTACKER);
//     let victim_address = Addr::unchecked(VICTIM);
//
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(1000u128),
//     };
//
//     let err = router
//         .execute_contract(
//             attacker_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap_err();
//     assert_eq!(
//         err.root_cause().to_string(),
//         "Initial stake amount must be more than 1000"
//     );
//
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(1001u128),
//     };
//
//     router
//         .execute_contract(
//             attacker_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();
//
//     let msg = Cw20ExecuteMsg::Transfer {
//         recipient: staking_instance.to_string(),
//         amount: Uint128::from(5000u128),
//     };
//
//     router
//         .execute_contract(
//             attacker_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();
//
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(2u128),
//     };
//
//     let err = router
//         .execute_contract(
//             victim_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(err.root_cause().to_string(), "Insufficient amount of Stake");
//
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };
//
//     router
//         .execute_contract(
//             victim_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();
// }

// fn mint_some_astro(router: &mut App, owner: Addr, astro_token_instance: Addr, to: &str) {
//     let msg = cw20::Cw20ExecuteMsg::Mint {
//         recipient: String::from(to),
//         amount: Uint128::from(10000u128),
//     };
//     let res = router
//         .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
//         .unwrap();
//     assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
//     assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
//     assert_eq!(
//         res.events[1].attributes[3],
//         attr("amount", Uint128::from(10000u128))
//     );
// }

// #[test]
// fn cw20receive_enter_and_leave() {
//     let mut router = mock_app();

//     let owner = Addr::unchecked("owner");

//     let (astro_token_instance, staking_instance, x_astro_token_instance) =
//         instantiate_contracts(&mut router, owner.clone());

//     // Mint 10000 ASTRO for Alice
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         ALICE,
//     );

//     let alice_address = Addr::unchecked(ALICE);

//     // Check if Alice's ASTRO balance is 100
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(10000u128)
//         }
//     );

//     // We can unstake ASTRO only by calling the xASTRO token.
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Leave {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };

//     let resp = router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap_err();
//     assert_eq!(resp.root_cause().to_string(), "Unauthorized");

//     // Tru to stake Alice's 1100 ASTRO for 1100 xASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(1100u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1100
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(100u128)
//         }
//     );

//     // Check if Alice's ASTRO balance is 8900
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(8900u128)
//         }
//     );

//     // Check if the staking contract's ASTRO balance is 1100
//     let msg = Cw20QueryMsg::Balance {
//         address: staking_instance.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1100u128)
//         }
//     );

//     // We can stake tokens only by calling the ASTRO token.
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };

//     let resp = router
//         .execute_contract(
//             alice_address.clone(),
//             x_astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap_err();
//     assert_eq!(resp.root_cause().to_string(), "Unauthorized");

//     // Try to unstake Alice's 10 xASTRO for 10 ASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Leave {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             x_astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 90
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(90u128)
//         }
//     );

//     // Check if Alice's ASTRO balance is 8910
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(8910u128)
//         }
//     );

//     // Check if the staking contract's ASTRO balance is 1090
//     let msg = Cw20QueryMsg::Balance {
//         address: staking_instance.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1090u128)
//         }
//     );

//     // Check if the staking contract's xASTRO balance is 1000
//     let msg = Cw20QueryMsg::Balance {
//         address: staking_instance.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1000u128)
//         }
//     );

//     let res: Uint128 = router
//         .wrap()
//         .query_wasm_smart(staking_instance.clone(), &QueryMsg::TotalDeposit {})
//         .unwrap();
//     assert_eq!(res.u128(), 1090);
//     let res: Uint128 = router
//         .wrap()
//         .query_wasm_smart(staking_instance, &QueryMsg::TotalShares {})
//         .unwrap();
//     assert_eq!(res.u128(), 1090);
// }

// #[test]
// fn should_not_allow_withdraw_more_than_what_you_have() {
//     let mut router = mock_app();

//     let owner = Addr::unchecked("owner");

//     let (astro_token_instance, staking_instance, x_astro_token_instance) =
//         instantiate_contracts(&mut router, owner.clone());

//     // Mint 10000 ASTRO for Alice
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         ALICE,
//     );
//     let alice_address = Addr::unchecked(ALICE);

//     // enter Alice's 2000 ASTRO for 1000 xASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(2000u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1000
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1000u128)
//         }
//     );

//     // Try to burn Alice's 2000 xASTRO and unstake
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Leave {}).unwrap(),
//         amount: Uint128::from(2000u128),
//     };

//     let res = router
//         .execute_contract(
//             alice_address.clone(),
//             x_astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap_err();

//     assert_eq!(
//         res.root_cause().to_string(),
//         "Cannot Sub with 1000 and 2000"
//     );
// }

// #[test]
// fn should_work_with_more_than_one_participant() {
//     let mut router = mock_app();

//     let owner = Addr::unchecked("owner");

//     let (astro_token_instance, staking_instance, x_astro_token_instance) =
//         instantiate_contracts(&mut router, owner.clone());

//     // Mint 10000 ASTRO for Alice
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         ALICE,
//     );
//     let alice_address = Addr::unchecked(ALICE);

//     // Mint 10000 ASTRO for Bob
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         BOB,
//     );
//     let bob_address = Addr::unchecked(BOB);

//     // Mint 10000 ASTRO for Carol
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         CAROL,
//     );
//     let carol_address = Addr::unchecked(CAROL);

//     // Stake Alice's 2000 ASTRO for 1000 xASTRO (subtract min liquid amount)
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(2000u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Stake Bob's 10 ASTRO for 10 xASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };

//     router
//         .execute_contract(bob_address.clone(), astro_token_instance.clone(), &msg, &[])
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1000
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1000u128)
//         }
//     );

//     // Check if Bob's xASTRO balance is 10
//     let msg = Cw20QueryMsg::Balance {
//         address: bob_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(10u128)
//         }
//     );

//     // Check if staking contract's ASTRO balance is 2010
//     let msg = Cw20QueryMsg::Balance {
//         address: staking_instance.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(2010u128)
//         }
//     );

//     // Staking contract gets 20 more ASTRO from external source
//     let msg = Cw20ExecuteMsg::Transfer {
//         recipient: staking_instance.to_string(),
//         amount: Uint128::from(20u128),
//     };
//     let res = router
//         .execute_contract(
//             carol_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();
//     assert_eq!(res.events[1].attributes[1], attr("action", "transfer"));
//     assert_eq!(res.events[1].attributes[2], attr("from", carol_address));
//     assert_eq!(
//         res.events[1].attributes[3],
//         attr("to", staking_instance.clone())
//     );
//     assert_eq!(
//         res.events[1].attributes[4],
//         attr("amount", Uint128::from(20u128))
//     );

//     // Stake Alice's 10 ASTRO for 9 xASTRO: 10*2010/2030 = 9
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(10u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1009
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1009u128)
//         }
//     );

//     // Check if Bob's xASTRO balance is 10
//     let msg = Cw20QueryMsg::Balance {
//         address: bob_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(10u128)
//         }
//     );

//     // Burn Bob's 5 xASTRO and unstake: gets 5*2040/2019 = 5 ASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Leave {}).unwrap(),
//         amount: Uint128::from(5u128),
//     };

//     router
//         .execute_contract(
//             bob_address.clone(),
//             x_astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1009
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1009u128)
//         }
//     );

//     // Check if Bob's xASTRO balance is 5
//     let msg = Cw20QueryMsg::Balance {
//         address: bob_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(5u128)
//         }
//     );

//     // Check if the staking contract's ASTRO balance is 52 (60 - 8 (Bob left 5 xASTRO))
//     let msg = Cw20QueryMsg::Balance {
//         address: staking_instance.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(2035u128)
//         }
//     );

//     // Check if Alice's ASTRO balance is 7990 (10000 minted - 2000 entered - 10 entered)
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(7990u128)
//         }
//     );

//     // Check if Bob's ASTRO balance is 9995 (10000 minted - 10 entered + 5 by leaving)
//     let msg = Cw20QueryMsg::Balance {
//         address: bob_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(9995u128)
//         }
//     );
// }

// #[test]
// fn should_not_allow_directly_burn_from_xastro() {
//     let mut router = mock_app();

//     let owner = Addr::unchecked("owner");

//     let (astro_token_instance, staking_instance, x_astro_token_instance) =
//         instantiate_contracts(&mut router, owner.clone());

//     // Mint 10000 ASTRO for Alice
//     mint_some_astro(
//         &mut router,
//         owner.clone(),
//         astro_token_instance.clone(),
//         ALICE,
//     );
//     let alice_address = Addr::unchecked(ALICE);

//     // enter Alice's 2000 ASTRO for 1000 xASTRO
//     let msg = Cw20ExecuteMsg::Send {
//         contract: staking_instance.to_string(),
//         msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
//         amount: Uint128::from(2000u128),
//     };

//     router
//         .execute_contract(
//             alice_address.clone(),
//             astro_token_instance.clone(),
//             &msg,
//             &[],
//         )
//         .unwrap();

//     // Check if Alice's xASTRO balance is 1000
//     let msg = Cw20QueryMsg::Balance {
//         address: alice_address.to_string(),
//     };
//     let res: Result<BalanceResponse, _> =
//         router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: x_astro_token_instance.to_string(),
//             msg: to_json_binary(&msg).unwrap(),
//         }));
//     assert_eq!(
//         res.unwrap(),
//         BalanceResponse {
//             balance: Uint128::from(1000u128)
//         }
//     );

//     // Try to burn directly
//     let res = router
//         .execute_contract(
//             alice_address.clone(),
//             x_astro_token_instance.clone(),
//             &Cw20ExecuteMsg::Burn {
//                 amount: Uint128::from(20u128),
//             },
//             &[],
//         )
//         .unwrap_err();
//     assert_eq!(res.root_cause().to_string(), "Unauthorized");
// }
