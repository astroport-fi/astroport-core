#![cfg(not(tarpaulin_include))]

use cosmwasm_std::{coin, Addr};
use cw_multi_test::{App, BankSudo, ContractWrapper, Executor};

use astroport::native_coin_registry::{CoinResponse, Config, ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport_native_coin_registry::error::ContractError;

fn mock_app() -> App {
    App::default()
}

fn store_native_registry_code(app: &mut App) -> u64 {
    let contract = Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ));

    app.store_code(contract)
}

#[test]
fn proper_initialization() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");

    let native_registry_code_id = store_native_registry_code(&mut app);
    let msg = InstantiateMsg {
        owner: owner.to_string(),
    };

    let native_registry_instance = app
        .instantiate_contract(
            native_registry_code_id,
            Addr::unchecked(owner.clone()),
            &msg,
            &[],
            "Precision registry contract",
            None,
        )
        .unwrap();

    let msg = QueryMsg::Config {};
    let config_res: Config = app
        .wrap()
        .query_wasm_smart(&native_registry_instance, &msg)
        .unwrap();

    assert_eq!(owner, config_res.owner);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let new_owner = String::from("new_owner");

    let native_registry_code_id = store_native_registry_code(&mut app);
    let msg = InstantiateMsg {
        owner: owner.to_string(),
    };

    let native_registry_instance = app
        .instantiate_contract(
            native_registry_code_id,
            Addr::unchecked(owner.clone()),
            &msg,
            &[],
            "Precision registry contract",
            None,
        )
        .unwrap();

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            native_registry_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            native_registry_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(
        Addr::unchecked("owner"),
        native_registry_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            native_registry_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            native_registry_instance.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap_err();
    // new_owner is not an owner yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        owner.clone(),
        native_registry_instance.clone(),
        &ExecuteMsg::DropOwnershipProposal {},
        &[],
    )
    .unwrap();

    // Try to claim ownership
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            native_registry_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner again
    app.execute_contract(
        Addr::unchecked("owner"),
        native_registry_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();
    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        native_registry_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: Config = app
        .wrap()
        .query_wasm_smart(&native_registry_instance, &msg)
        .unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn try_add_and_remove_native_tokens() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");

    let native_registry_code_id = store_native_registry_code(&mut app);
    let msg = InstantiateMsg {
        owner: owner.to_string(),
    };

    let native_registry_instance = app
        .instantiate_contract(
            native_registry_code_id,
            Addr::unchecked(owner.clone()),
            &msg,
            &[],
            "Precision registry contract",
            None,
        )
        .unwrap();

    let msg = ExecuteMsg::Add {
        native_coins: vec![
            ("ULUNA".to_string(), 18),
            ("USDT".to_string(), 10),
            ("usdc".to_string(), 0),
            ("usdc".to_string(), 1),
        ],
    };

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            native_registry_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    let err = app
        .execute_contract(
            Addr::unchecked("owner"),
            native_registry_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Duplicate coins are provided");

    let msg = ExecuteMsg::Add {
        native_coins: vec![
            ("ULUNA".to_string(), 18),
            ("USDT".to_string(), 10),
            ("usdc".to_string(), 3),
        ],
    };

    app.execute_contract(
        Addr::unchecked("owner"),
        native_registry_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // query asset info by denominator name
    let coin_decimals: u8 = app
        .wrap()
        .query_wasm_smart(
            &native_registry_instance,
            &QueryMsg::NativeToken {
                denom: "usdc".to_string(),
            },
        )
        .unwrap();

    assert_eq!(3, coin_decimals);

    // query asset info by denominator name
    let config_res: Vec<CoinResponse> = app
        .wrap()
        .query_wasm_smart(
            &native_registry_instance,
            &QueryMsg::NativeTokens {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(
        vec![
            CoinResponse {
                denom: "ULUNA".to_string(),
                decimals: 18
            },
            CoinResponse {
                denom: "USDT".to_string(),
                decimals: 10
            },
            CoinResponse {
                denom: "usdc".to_string(),
                decimals: 3
            }
        ],
        config_res
    );

    // query asset info by denominator name
    let config_res: Vec<CoinResponse> = app
        .wrap()
        .query_wasm_smart(
            &native_registry_instance,
            &QueryMsg::NativeTokens {
                start_after: Some("USDT".to_string()),
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(
        vec![CoinResponse {
            denom: "usdc".to_string(),
            decimals: 3
        }],
        config_res
    );

    let msg = ExecuteMsg::Remove {
        native_coins: vec!["usdc".to_string(), "usdc".to_string()],
    };

    let err = app
        .execute_contract(
            Addr::unchecked("owner"),
            native_registry_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicateCoins {}
    );

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            native_registry_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    let err = app
        .execute_contract(
            Addr::unchecked("owner"),
            native_registry_instance.clone(),
            &ExecuteMsg::Remove {
                native_coins: vec!["foo_coin".to_string()],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::CoinDoesNotExist("foo_coin".to_string())
    );

    app.execute_contract(
        Addr::unchecked("owner"),
        native_registry_instance.clone(),
        &ExecuteMsg::Remove {
            native_coins: vec!["usdc".to_string()],
        },
        &[],
    )
    .unwrap();

    // query asset info by denominator name
    let config_res: Vec<CoinResponse> = app
        .wrap()
        .query_wasm_smart(
            &native_registry_instance,
            &QueryMsg::NativeTokens {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(
        vec![
            CoinResponse {
                denom: "ULUNA".to_string(),
                decimals: 18
            },
            CoinResponse {
                denom: "USDT".to_string(),
                decimals: 10
            }
        ],
        config_res
    );
}

#[test]
fn test_permissionless_add() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");

    let native_registry_code_id = store_native_registry_code(&mut app);
    let msg = InstantiateMsg {
        owner: owner.to_string(),
    };

    let native_registry_instance = app
        .instantiate_contract(
            native_registry_code_id,
            owner.clone(),
            &msg,
            &[],
            "label",
            None,
        )
        .unwrap();

    let err = app
        .execute_contract(
            Addr::unchecked("random"),
            native_registry_instance.clone(),
            &ExecuteMsg::Register {
                native_coins: vec![("utoken".to_string(), 6)],
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::MustSendCoin("utoken".to_string())
    );

    app.sudo(
        BankSudo::Mint {
            to_address: "random".to_string(),
            amount: vec![coin(1, "ufoo"), coin(1, "adydx"), coin(100000, "utoken")],
        }
        .into(),
    )
    .unwrap();

    let err = app
        .execute_contract(
            Addr::unchecked("random"),
            native_registry_instance.clone(),
            &ExecuteMsg::Register {
                native_coins: vec![
                    ("ufoo".to_string(), 6),
                    ("adydx".to_string(), 18),
                    ("utoken".to_string(), 228),
                ],
            },
            &[coin(1, "ufoo"), coin(1, "adydx"), coin(1, "utoken")],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidDecimals {
            denom: "utoken".to_string(),
            decimals: 228,
        }
    );

    let err = app
        .execute_contract(
            Addr::unchecked("random"),
            native_registry_instance.clone(),
            &ExecuteMsg::Register {
                native_coins: vec![
                    ("ufoo".to_string(), 6),
                    ("adydx".to_string(), 18),
                    ("utoken".to_string(), 8),
                    ("utoken".to_string(), 6),
                ],
            },
            &[coin(1, "ufoo"), coin(1, "adydx"), coin(2, "utoken")],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicateCoins {}
    );

    app.execute_contract(
        Addr::unchecked("random"),
        native_registry_instance.clone(),
        &ExecuteMsg::Register {
            native_coins: vec![
                ("ufoo".to_string(), 6),
                ("adydx".to_string(), 18),
                ("utoken".to_string(), 8),
            ],
        },
        &[coin(1, "ufoo"), coin(1, "adydx"), coin(1, "utoken")],
    )
    .unwrap();

    // Ensure coin registry doesn't hold these coins
    let foo_bal = app
        .wrap()
        .query_balance(&native_registry_instance, "ufoo")
        .unwrap()
        .amount
        .u128();
    assert_eq!(foo_bal, 0);
    let user_foo_bal = app
        .wrap()
        .query_balance(&Addr::unchecked("random"), "ufoo")
        .unwrap()
        .amount
        .u128();
    assert_eq!(user_foo_bal, 1);

    // Try to update existing coin
    let err = app
        .execute_contract(
            Addr::unchecked("random"),
            native_registry_instance.clone(),
            &ExecuteMsg::Register {
                native_coins: vec![("utoken".to_string(), 6)],
            },
            &[coin(1, "utoken")],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::CoinAlreadyExists("utoken".to_string())
    );

    // However, owner can update the coin
    app.execute_contract(
        owner.clone(),
        native_registry_instance.clone(),
        &ExecuteMsg::Add {
            native_coins: vec![("utoken".to_string(), 6)],
        },
        &[],
    )
    .unwrap();

    let coin_decimals: u8 = app
        .wrap()
        .query_wasm_smart(
            &native_registry_instance,
            &QueryMsg::NativeToken {
                denom: "utoken".to_string(),
            },
        )
        .unwrap();
    assert_eq!(coin_decimals, 6);
}
