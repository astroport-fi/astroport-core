use astroport::fee_granter::{Config, ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport_fee_granter::contract::{execute, instantiate};
use astroport_fee_granter::error::ContractError;
use astroport_fee_granter::query::query;
use astroport_fee_granter::state::MAX_ADMINS;
use cosmwasm_std::{coins, Addr, Empty};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

fn fee_granter_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

const GAS_DENOM: &str = "inj";

#[test]
fn test_init() {
    let owner = Addr::unchecked("owner");
    let mut app = App::new(|router, _, store| {
        router
            .bank
            .init_balance(store, &owner, coins(1000000, GAS_DENOM))
            .unwrap();
    });

    let fee_granter_code_id = app.store_code(fee_granter_contract());
    let mut init_msg = InstantiateMsg {
        owner: owner.to_string(),
        admins: vec![
            "admin1".to_string(),
            "admin2".to_string(),
            "admin3".to_string(),
        ],
        gas_denom: GAS_DENOM.to_string(),
    };
    let err = app
        .instantiate_contract(
            fee_granter_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Test contract",
            None,
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: Maximum allowed number of admins is {MAX_ADMINS}")
    );
    init_msg.admins = vec!["admin1".to_string(), "admin2".to_string()];
    let fee_granter = app
        .instantiate_contract(
            fee_granter_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Test contract",
            None,
        )
        .unwrap();

    app.send_tokens(owner.clone(), fee_granter.clone(), &coins(10, GAS_DENOM))
        .unwrap();

    let inj_bal = app
        .wrap()
        .query_balance(&fee_granter, GAS_DENOM)
        .unwrap()
        .amount
        .u128();

    assert_eq!(inj_bal, 10);

    let inj_bal_before = app
        .wrap()
        .query_balance(&owner, GAS_DENOM)
        .unwrap()
        .amount
        .u128();

    app.execute_contract(
        owner.clone(),
        fee_granter.clone(),
        &ExecuteMsg::TransferCoins {
            amount: 5u128.into(),
            receiver: None,
        },
        &[],
    )
    .unwrap();

    let inj_bal_after = app
        .wrap()
        .query_balance(&owner, GAS_DENOM)
        .unwrap()
        .amount
        .u128();

    assert_eq!(inj_bal_after - inj_bal_before, 5);

    let receiver_addr = "receiver".to_string();
    app.execute_contract(
        owner,
        fee_granter,
        &ExecuteMsg::TransferCoins {
            amount: 5u128.into(),
            receiver: Some(receiver_addr.clone()),
        },
        &[],
    )
    .unwrap();

    let inj_bal_receiver = app
        .wrap()
        .query_balance(&receiver_addr, GAS_DENOM)
        .unwrap()
        .amount
        .u128();

    assert_eq!(inj_bal_receiver, 5);
}

#[test]
fn test_update_admins() {
    let owner = Addr::unchecked("owner");
    let admin = Addr::unchecked("admin");
    let mut app = App::new(|router, _, store| {
        router
            .bank
            .init_balance(store, &owner, coins(1000000, GAS_DENOM))
            .unwrap();
    });

    let fee_granter_code_id = app.store_code(fee_granter_contract());
    let fee_granter = app
        .instantiate_contract(
            fee_granter_code_id,
            owner.clone(),
            &InstantiateMsg {
                owner: owner.to_string(),
                admins: vec![admin.to_string()],
                gas_denom: GAS_DENOM.to_string(),
            },
            &[],
            "Test contract",
            None,
        )
        .unwrap();

    app.send_tokens(owner.clone(), admin.clone(), &coins(10, GAS_DENOM))
        .unwrap();
    app.send_tokens(owner.clone(), fee_granter.clone(), &coins(5, GAS_DENOM))
        .unwrap();

    // Admin can only create, revoke grants and transfer coins
    app.execute_contract(
        admin.clone(),
        fee_granter.clone(),
        &ExecuteMsg::TransferCoins {
            amount: 5u128.into(),
            receiver: None,
        },
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            owner.clone(),
            fee_granter.clone(),
            &ExecuteMsg::UpdateAdmins {
                add: vec!["admin2".to_string(), "admin3".to_string()],
                remove: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: Maximum allowed number of admins is {MAX_ADMINS}")
    );

    // Stargate messages are not implemented in cw-multitest thus we assert that we receive exact cw-multitest error
    let err = app
        .execute_contract(
            admin.clone(),
            fee_granter.clone(),
            &ExecuteMsg::Grant {
                grantee_contract: "test".to_string(),
                amount: 10u128.into(),
                bypass_amount_check: false,
            },
            &coins(10, GAS_DENOM),
        )
        .unwrap_err();
    assert!(
        err.root_cause()
            .to_string()
            .contains("Cannot execute Stargate"),
        "{err}"
    );

    // only owner is able to update admins
    let err = app
        .execute_contract(
            admin.clone(),
            fee_granter.clone(),
            &ExecuteMsg::UpdateAdmins {
                add: vec!["admin2".to_string()],
                remove: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let err = app
        .execute_contract(
            owner.clone(),
            fee_granter.clone(),
            &ExecuteMsg::UpdateAdmins {
                add: vec![admin.to_string()],
                remove: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: Admin {} already exists", admin)
    );

    app.execute_contract(
        owner.clone(),
        fee_granter.clone(),
        &ExecuteMsg::UpdateAdmins {
            add: vec!["admin2".to_string()],
            remove: vec![],
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner,
        fee_granter,
        &ExecuteMsg::UpdateAdmins {
            add: vec![],
            remove: vec!["admin2".to_string(), "random".to_string()], // random is not admin thus it should be ignored
        },
        &[],
    )
    .unwrap();
}

#[test]
fn test_change_ownership() {
    let owner = Addr::unchecked("owner");
    let admin = Addr::unchecked("admin");
    let mut app = App::default();

    let fee_granter_code_id = app.store_code(fee_granter_contract());
    let fee_granter = app
        .instantiate_contract(
            fee_granter_code_id,
            owner.clone(),
            &InstantiateMsg {
                owner: owner.to_string(),
                admins: vec![admin.to_string()],
                gas_denom: GAS_DENOM.to_string(),
            },
            &[],
            "Test contract",
            None,
        )
        .unwrap();

    let new_owner = Addr::unchecked("new_owner".to_string());

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = app
        .execute_contract(Addr::unchecked("not_owner"), fee_granter.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            new_owner.clone(),
            fee_granter.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(owner, fee_granter.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            fee_granter.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        new_owner.clone(),
        fee_granter.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    let res: Config = app
        .wrap()
        .query_wasm_smart(&fee_granter, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.owner, new_owner)
}
