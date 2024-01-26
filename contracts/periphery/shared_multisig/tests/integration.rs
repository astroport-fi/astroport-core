#![cfg(not(tarpaulin_include))]

use astroport::asset::{Asset, AssetInfo};
use astroport::generator::PendingTokenResponse;
use cosmwasm_std::{to_json_binary, Addr, Coin, CosmosMsg, Decimal, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use cw3::{Status, Vote, VoteInfo, VoteListResponse, VoteResponse};
use cw_utils::{Duration, ThresholdResponse};
use std::{cell::RefCell, rc::Rc};

use astroport::shared_multisig::{ExecuteMsg, PoolType, ProvideParams};

use astroport_mocks::cw_multi_test::{App, Executor};
use astroport_mocks::shared_multisig::MockSharedMultisigBuilder;
use astroport_mocks::{astroport_address, MockFactoryBuilder, MockGeneratorBuilder};

fn mock_app(owner: &Addr, coins: Option<Vec<Coin>>) -> App {
    let app = App::new(|router, _, storage| {
        // initialization moved to App construction
        router
            .bank
            .init_balance(storage, &owner, coins.unwrap_or_default())
            .unwrap();
    });

    app
}

const OWNER: &str = "owner";
const MANAGER1: &str = "manager1";
const MANAGER2: &str = "manager2";
const CHEATER: &str = "cheater";

#[test]
fn proper_initialization() {
    let manager2 = Addr::unchecked("manager2");
    let manager1 = Addr::unchecked("manager1");

    let router = Rc::new(RefCell::new(App::default()));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    let config_res = shared_multisig.query_config().unwrap();

    assert_eq!(manager2, config_res.manager2);
    assert_eq!(manager1, config_res.manager1);
    assert_eq!(Duration::Height(3), config_res.max_voting_period);
    assert_eq!(
        ThresholdResponse::AbsoluteCount {
            weight: 2,
            total_weight: 2
        },
        config_res.threshold
    );
}

#[test]
fn check_update_manager2() {
    let manager1 = Addr::unchecked("manager1");
    let manager2 = Addr::unchecked("manager2");
    let new_manager = Addr::unchecked("new_manager");

    let router = Rc::new(RefCell::new(App::default()));
    let factory = MockFactoryBuilder::new(&router).instantiate();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // New manager
    let msg = ExecuteMsg::ProposeNewManager2 {
        new_manager: "new_manager".to_string(),
        expires_in: 100, // seconds
    };

    let err = router
        .borrow_mut()
        .execute_contract(manager1.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager1 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Try to propose new manager2
    router
        .borrow_mut()
        .execute_contract(manager2.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap();

    // Claim from manager1
    let err = router
        .borrow_mut()
        .execute_contract(
            manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager2 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop manager1 proposal
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::DropManager1Proposal {},
            &[],
        )
        .unwrap_err();

    // new_manager is not an manager1 yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    router
        .borrow_mut()
        .execute_contract(
            manager2.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::DropManager2Proposal {},
            &[],
        )
        .unwrap();

    // Try to claim manager2
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager2 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new manager again
    router
        .borrow_mut()
        .execute_contract(manager2.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap();

    // Claim manager2
    router
        .borrow_mut()
        .execute_contract(
            new_manager.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager2 {},
            &[],
        )
        .unwrap();

    // Let's query the contract state
    let res = shared_multisig.query_config().unwrap();

    assert_eq!(res.manager2, new_manager);
    assert_eq!(res.manager1, manager1);
}

#[test]
fn check_update_manager1() {
    let manager2 = Addr::unchecked("manager2");
    let manager1 = Addr::unchecked("manager1");
    let new_manager1 = Addr::unchecked("new_manager1");

    let router = Rc::new(RefCell::new(App::default()));
    let factory = MockFactoryBuilder::new(&router).instantiate();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // New manager1
    let msg = ExecuteMsg::ProposeNewManager1 {
        new_manager: new_manager1.to_string(),
        expires_in: 100, // seconds
    };

    let err = router
        .borrow_mut()
        .execute_contract(manager2.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager1 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Try to propose new manager1
    router
        .borrow_mut()
        .execute_contract(manager1.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap();

    // Claim from manager2
    let err = router
        .borrow_mut()
        .execute_contract(
            manager2.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager1 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop manager1 proposal
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::DropManager1Proposal {},
            &[],
        )
        .unwrap_err();

    // new_manager1 is not an manager1 yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    router
        .borrow_mut()
        .execute_contract(
            manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::DropManager1Proposal {},
            &[],
        )
        .unwrap();

    // Try to claim manager1
    let err = router
        .borrow_mut()
        .execute_contract(
            new_manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager1 {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new manager1 again
    router
        .borrow_mut()
        .execute_contract(manager1.clone(), shared_multisig.address.clone(), &msg, &[])
        .unwrap();

    // Claim manager1
    router
        .borrow_mut()
        .execute_contract(
            new_manager1.clone(),
            shared_multisig.address.clone(),
            &ExecuteMsg::ClaimManager1 {},
            &[],
        )
        .unwrap();

    // Let's query the contract state
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.manager2, manager2);
    assert_eq!(res.manager1, new_manager1);
}

#[test]
fn test_proposal() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let cheater = Addr::unchecked(CHEATER);
    let astroport = astroport_address();

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &astroport,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let pcl = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    let setup_pools_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: shared_multisig.address.to_string(),
        msg: to_json_binary(&ExecuteMsg::SetupPools {
            target_pool: None,
            migration_pool: Some(pcl.address.to_string()),
        })
        .unwrap(),
        funds: vec![],
    });

    // try to propose from cheater
    let err = shared_multisig
        .propose(&cheater, vec![setup_pools_msg.clone()])
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // try to propose from manager1
    shared_multisig
        .propose(&manager1, vec![setup_pools_msg.clone()])
        .unwrap();

    // Try to vote from cheater
    let err = shared_multisig.vote(&cheater, 1, Vote::Yes).unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // Try to execute from cheater
    let err = shared_multisig.execute(&cheater, 1).unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // Try to execute from manager1
    let err = shared_multisig.execute(&manager1, 1).unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // Check manager1 vote
    let res = shared_multisig.query_vote(1, &manager1).unwrap();
    assert_eq!(
        res,
        VoteResponse {
            vote: Some(VoteInfo {
                proposal_id: 1,
                voter: manager1.to_string(),
                vote: Vote::Yes,
                weight: 1
            }),
        }
    );

    // Check manager2 vote
    let res = shared_multisig.query_vote(1, &manager2).unwrap();
    assert_eq!(res.vote, None);

    // Try to vote from manager2
    shared_multisig.vote(&manager2, 1, Vote::No).unwrap();

    // Check manager2 vote
    let res = shared_multisig.query_vote(1, &manager2).unwrap();
    assert_eq!(
        res,
        VoteResponse {
            vote: Some(VoteInfo {
                proposal_id: 1,
                voter: manager2.to_string(),
                vote: Vote::No,
                weight: 1
            })
        }
    );

    // Check manager2 vote
    let res = shared_multisig.query_votes(1).unwrap();
    assert_eq!(
        res,
        VoteListResponse {
            votes: vec![
                VoteInfo {
                    proposal_id: 1,
                    voter: "manager1".to_string(),
                    vote: Vote::Yes,
                    weight: 1
                },
                VoteInfo {
                    proposal_id: 1,
                    voter: "manager2".to_string(),
                    vote: Vote::No,
                    weight: 1
                }
            ]
        }
    );

    // Try to vote from Manager2
    let err = shared_multisig.vote(&manager2, 1, Vote::Yes).unwrap_err();
    assert_eq!(
        "Already voted on this proposal",
        err.root_cause().to_string()
    );

    // try to propose the second proposal from manager1
    shared_multisig
        .propose(&manager1, vec![setup_pools_msg.clone()])
        .unwrap();

    router.borrow_mut().update_block(|b| {
        b.height += 4;
    });

    // check that the first proposal is rejected
    let res = shared_multisig.query_proposal(1).unwrap();
    assert_eq!(res.status, Status::Rejected);

    // Try to vote from Manager2
    let err = shared_multisig.vote(&manager2, 2, Vote::Yes).unwrap_err();
    assert_eq!(
        "Proposal voting period has expired",
        err.root_cause().to_string()
    );

    // try to execute the second proposal from the cheater
    let err = shared_multisig.execute(&cheater, 2).unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // check that the second proposal is rejected
    let res = shared_multisig.query_proposal(2).unwrap();
    assert_eq!(res.status, Status::Rejected);

    // Try to setup max voting period config from Manager2
    let err = shared_multisig
        .setup_max_voting_period(&manager2, Duration::Height(10))
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // Try to setup max voting period config direct from multisig
    shared_multisig
        .setup_max_voting_period(&shared_multisig.address, Duration::Height(10))
        .unwrap();

    // check configuration
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.max_voting_period, Duration::Height(10));

    // try to propose from manager1
    shared_multisig
        .propose(&manager1, vec![setup_pools_msg.clone()])
        .unwrap();

    // Try to vote from Manager2
    shared_multisig.vote(&manager2, 3, Vote::Yes).unwrap();

    // Try to execute the third proposal
    shared_multisig.execute(&manager2, 3).unwrap();

    // check configuration
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.target_pool, None);
    assert_eq!(res.migration_pool, Some(pcl.address));
}

#[test]
fn test_transfer() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let owner = Addr::unchecked(OWNER);
    let recipient = Addr::unchecked("recipient");

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &owner,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // Sends tokens to the multisig
    shared_multisig
        .send_tokens(
            &owner,
            Some(vec![
                Coin {
                    denom: denom1.clone(),
                    amount: Uint128::new(200_000_000u128),
                },
                Coin {
                    denom: denom2.clone(),
                    amount: Uint128::new(200_000_000u128),
                },
                Coin {
                    denom: denom3.clone(),
                    amount: Uint128::new(300_000_000u128),
                },
            ]),
            None,
        )
        .unwrap();

    // Check the recipient's balance utrn
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), denom1.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, denom1.clone());

    // Check the recipient's balance
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), denom2.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, denom2.clone());

    // Check the recipient's balance
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), denom3.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, denom3);

    // Check the holder's balance
    let res = shared_multisig
        .query_native_balance(None, denom1.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::new(200_000_000));
    assert_eq!(res.denom, denom1);

    // Check the holder's balance
    let res = shared_multisig
        .query_native_balance(None, denom2.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::new(200_000_000));
    assert_eq!(res.denom, denom2);

    // Check the holder's balance
    let res = shared_multisig
        .query_native_balance(None, denom3.as_str())
        .unwrap();
    assert_eq!(res.amount, Uint128::new(300_000_000));
    assert_eq!(res.denom, denom3);

    // try to transfer when rage quit is not started yet
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom1.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        "Operation is unavailable. Rage quit is not started",
        err.root_cause().to_string()
    );

    // try to transfer when rage quit is not started yet
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom2.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        "Operation is unavailable. Rage quit is not started",
        err.root_cause().to_string()
    );

    // try to transfer denom3 when rage quit is not started yet
    shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // try to update config from manager1
    shared_multisig.start_rage_quit(&manager2).unwrap();

    // try to transfer denom1 from manager2
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom1.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        "Unauthorized: manager2 cannot transfer untrn",
        err.root_cause().to_string()
    );

    // try to transfer denom1 from manager1
    shared_multisig
        .transfer(
            &manager1,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom1.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // try to transfer denom2 from manager1
    let err = shared_multisig
        .transfer(
            &manager1,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom2.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        "Unauthorized: manager1 cannot transfer ibc/astro",
        err.root_cause().to_string()
    );

    // try to transfer denom2 from manager2
    shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom2.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // try to transfer usdt from manager1
    shared_multisig
        .transfer(
            &manager1,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // try to transfer usdt from manager2
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Insufficient balance for: manager2. Available balance: 50000000"
    );

    // try to transfer usdt from manager2
    let err = shared_multisig
        .transfer(
            &manager1,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Insufficient balance for: manager1. Available balance: 50000000"
    );

    // try to transfer usdt from manager2
    shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(50_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // try to transfer usdt from manager2
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(50_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Insufficient balance for: manager2. Available balance: 0"
    );

    shared_multisig
        .transfer(
            &manager1,
            Asset {
                info: AssetInfo::NativeToken {
                    denom: denom3.to_string(),
                },
                amount: Uint128::new(50_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap();

    // Check the recipient's balance denom1
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), &denom1)
        .unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, denom1);

    // Check the recipient's balance denom2
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), &denom2)
        .unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, denom2);

    // Check the recipient's balance denom3
    let res = shared_multisig
        .query_native_balance(Some(recipient.as_str()), &denom3)
        .unwrap();
    assert_eq!(res.amount, Uint128::new(300_000_000));
    assert_eq!(res.denom, denom3);

    // Check the holder's balance
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, denom1);

    // Check the holder's balance
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, denom2);

    // Check the holder's balance
    let res = shared_multisig.query_native_balance(None, &denom3).unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, denom3);
}

#[test]
fn test_target_pool() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let owner = Addr::unchecked(OWNER);
    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &owner,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let pcl = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    let pcl_pair_info = pcl.pair_info();
    assert_eq!(
        pcl_pair_info.asset_infos,
        vec![
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ]
    );

    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&owner, None, None).unwrap();

    // try to provide from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Target pool is not set");

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(pcl.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(pcl.address));

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // try to withdraw from target
    let err = shared_multisig.withdraw(&manager1, None, None).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // Check the holder's balance for denom1
    let denom1_before = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom1.clone());

    // Check the holder's balance for denom2
    let denom1_before = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(99_999_000));

    // deregister the target pool
    factory
        .deregister_pair(&[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ])
        .unwrap();

    // create the migration pool
    let pcl_2 = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    // Direct set up migration pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            None,
            Some(pcl_2.address.to_string()),
        )
        .unwrap();

    // try to provide from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Migration pool is already set"
    );

    // try to withdraw from target pool
    shared_multisig.withdraw(&manager2, None, None).unwrap();

    // Check the holder's balance for denom1
    let denom1_before = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(899_998_999));
    assert_eq!(denom1_before.denom, denom1.clone());

    // Check the holder's balance for denom2
    let denom1_before = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(899_998_999));
    assert_eq!(denom1_before.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::zero());

    // try to update config from manager1
    shared_multisig.start_rage_quit(&manager2).unwrap();

    // check if rage quit started
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.rage_quit_started, true);

    // check if rage quit cannot be set back to false
    let err = shared_multisig.start_rage_quit(&manager2).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );

    // try to provide after rage quit started
    let err = shared_multisig
        .provide(&manager2, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );
}

#[test]
fn test_provide_withdraw_pcl() {
    let astroport = astroport_address();
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let recipient = Addr::unchecked("recipient");

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &astroport,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let pcl = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    let pcl_pair_info = pcl.pair_info();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(pcl.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(pcl.address));

    // try to provide from recipient
    let err = shared_multisig
        .provide(&recipient, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // try to provide without funds on multisig from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Asset balance mismatch between the argument and the \
    Multisig balance. Available Multisig balance for untrn: 0"
    );

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&astroport, None, None).unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom2.clone());

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // send tokens to the recipient
    shared_multisig
        .send_tokens(&astroport, None, Some(recipient.clone()))
        .unwrap();

    // try to swap tokens
    for _ in 0..10 {
        shared_multisig
            .swap(
                &recipient,
                &pcl_pair_info.contract_addr,
                &denom1,
                10_000_000,
                None,
                None,
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
            )
            .unwrap();

        router.borrow_mut().update_block(|b| {
            b.height += 1200;
            b.time = b.time.plus_seconds(3600);
        });

        shared_multisig
            .swap(
                &recipient,
                &pcl_pair_info.contract_addr,
                &denom2,
                15_000_000,
                None,
                None,
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
            )
            .unwrap();

        router.borrow_mut().update_block(|b| {
            b.height += 100;
        });

        // try to provide from manager2
        shared_multisig
            .provide(
                &manager2,
                PoolType::Target,
                Some(vec![
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: denom1.clone(),
                        },
                        amount: Uint128::new(10_000_000),
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: denom2.clone(),
                        },
                        amount: Uint128::new(10_000_000),
                    },
                ]),
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
                None,
            )
            .unwrap();
    }

    router.borrow_mut().update_block(|b| {
        b.time = b.time.plus_seconds(86400 * 7);
    });

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom2.clone());

    // try to provide from manager2
    shared_multisig
        .provide(
            &manager2,
            PoolType::Target,
            None,
            Some(Decimal::from_ratio(5u128, 10u128)),
            None,
            None,
        )
        .unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(600_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(600_000_000));
    assert_eq!(res.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(301_118_256));
}

#[test]
fn test_provide_withdraw_xyk() {
    let astroport = astroport_address();
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let recipient = Addr::unchecked("recipient");

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &astroport,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let xyk = factory.instantiate_xyk_pair(&[
        AssetInfo::NativeToken {
            denom: denom1.clone(),
        },
        AssetInfo::NativeToken {
            denom: denom2.clone(),
        },
    ]);

    let xyk_pair_info = xyk.pair_info().unwrap();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(xyk.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(xyk.address));

    // try to provide from recipient
    let err = shared_multisig
        .provide(&recipient, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // try to provide without funds on multisig from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Asset balance mismatch between the argument and the \
    Multisig balance. Available Multisig balance for untrn: 0"
    );

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&astroport, None, None).unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom2.clone());

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // send tokens to the recipient
    shared_multisig
        .send_tokens(&astroport, None, Some(recipient.clone()))
        .unwrap();

    // try to swap tokens
    for _ in 0..10 {
        shared_multisig
            .swap(
                &recipient,
                &xyk_pair_info.contract_addr,
                &denom1,
                10_000_000,
                None,
                None,
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
            )
            .unwrap();

        router.borrow_mut().update_block(|b| {
            b.height += 1400;
        });

        shared_multisig
            .swap(
                &recipient,
                &xyk_pair_info.contract_addr,
                &denom2,
                15_000_000,
                None,
                None,
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
            )
            .unwrap();

        router.borrow_mut().update_block(|b| {
            b.height += 100;
        });

        // try to provide from manager2
        shared_multisig
            .provide(
                &manager2,
                PoolType::Target,
                Some(vec![
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: denom1.clone(),
                        },
                        amount: Uint128::new(10_000_000),
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: denom2.clone(),
                        },
                        amount: Uint128::new(10_000_000),
                    },
                ]),
                Some(Decimal::from_ratio(5u128, 10u128)),
                None,
                None,
            )
            .unwrap();
    }

    router.borrow_mut().update_block(|b| {
        b.height += 500;
        b.time = b.time.plus_seconds(86400);
    });

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom2.clone());

    // try to provide from manager2
    shared_multisig
        .provide(
            &manager2,
            PoolType::Target,
            None,
            Some(Decimal::from_ratio(5u128, 10u128)),
            None,
            None,
        )
        .unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(600_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(600_000_000));
    assert_eq!(res.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(263_078_132));
}

#[test]
fn test_provide_to_both_pools() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let owner = Addr::unchecked(OWNER);
    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &owner,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let generator = MockGeneratorBuilder::new(&router).instantiate();
    let factory = generator.factory();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![
        (denom1.to_owned(), 6),
        (denom2.to_owned(), 6),
        (denom3.to_owned(), 6),
    ]);

    let pcl_target = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    let shared_multisig = MockSharedMultisigBuilder::new(&router).instantiate(
        &factory.address,
        Some(generator.address),
        None,
    );

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&owner, None, None).unwrap();

    // try to provide from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Target pool is not set");

    // try to provide from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Migration, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(pcl_target.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(pcl_target.address.clone()));
    assert_eq!(config.migration_pool, None);

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // try to withdraw from target
    let err = shared_multisig.withdraw(&manager1, None, None).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // try to withdraw from migration
    let err = shared_multisig.withdraw(&manager2, None, None).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // try to update config from manager1
    let err = shared_multisig
        .complete_target_pool_migration(&manager2)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // try to update config from manager1
    shared_multisig.start_rage_quit(&manager2).unwrap();

    // check if rage quit started
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.rage_quit_started, true);

    // check if rage quit cannot be set back to false
    let err = shared_multisig.start_rage_quit(&manager2).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );

    // try to provide after rage quit started in target pool
    let err = shared_multisig
        .provide(&manager2, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );
}

#[test]
fn test_transfer_lp_tokens() {
    let astroport = astroport_address();
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let cheater = Addr::unchecked(CHEATER);
    let recipient = Addr::unchecked("recipient");

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &astroport,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let pcl = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );

    let pcl_pair_info = pcl.pair_info();
    let shared_multisig =
        MockSharedMultisigBuilder::new(&router).instantiate(&factory.address, None, None);

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&astroport, None, None).unwrap();

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(pcl.address.to_string()),
            None,
        )
        .unwrap();

    // try to provide from recipient
    let err = shared_multisig
        .provide(&recipient, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(99_999_000));

    // Check the recipient's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, Some(recipient.clone()))
        .unwrap();
    assert_eq!(res.balance, Uint128::zero());

    // try to transfer LP tokens through transfer endpoint
    let err = shared_multisig
        .transfer(
            &manager2,
            Asset {
                info: AssetInfo::Token {
                    contract_addr: pcl_pair_info.liquidity_token.clone(),
                },
                amount: Uint128::new(100_000_000),
            },
            Some(recipient.to_string()),
        )
        .unwrap_err();
    assert_eq!(
        "Unauthorized: manager2 cannot transfer contract3",
        err.root_cause().to_string()
    );

    // create proposal message for transfer LP tokens to the recipient
    let lp_transfer_amount = Uint128::new(10_000_000);
    let transfer_lp_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pcl_pair_info.liquidity_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
            recipient: recipient.to_string(),
            amount: lp_transfer_amount,
        })
        .unwrap(),
        funds: vec![],
    });

    // try to propose from cheater
    let err = shared_multisig
        .propose(&cheater, vec![transfer_lp_msg.clone()])
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // try to propose from manager1
    shared_multisig
        .propose(&manager1, vec![transfer_lp_msg.clone()])
        .unwrap();

    // Try to vote from manager2
    shared_multisig.vote(&manager2, 1, Vote::Yes).unwrap();

    // Try to execute the third proposal
    shared_multisig.execute(&manager2, 1).unwrap();

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(89_999_000));

    // Check the recipient's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, Some(recipient))
        .unwrap();
    assert_eq!(res.balance, Uint128::new(10_000_000));
}

#[test]
fn test_end_migrate_from_target_to_migration_pool() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let owner = Addr::unchecked(OWNER);
    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &owner,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let generator = MockGeneratorBuilder::new(&router).instantiate();
    let factory = generator.factory();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let xyk_pool = factory.instantiate_xyk_pair(&[
        AssetInfo::NativeToken {
            denom: denom1.clone(),
        },
        AssetInfo::NativeToken {
            denom: denom2.clone(),
        },
    ]);

    let xyk_pair_info = xyk_pool.pair_info().unwrap();
    assert_eq!(
        xyk_pair_info.asset_infos,
        vec![
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ]
    );

    let shared_multisig = MockSharedMultisigBuilder::new(&router).instantiate(
        &factory.address,
        Some(generator.address),
        None,
    );

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&owner, None, None).unwrap();

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(xyk_pool.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(xyk_pool.address.clone()));

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // try to withdraw from target
    let err = shared_multisig.withdraw(&manager1, None, None).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Migration pool is not set");

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(99_999_000));

    // deregister the target pool
    factory
        .deregister_pair(&[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ])
        .unwrap();

    // create the migration pool
    let pcl_pool = factory.instantiate_concentrated_pair(
        &[
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ],
        None,
    );
    let pcl_pair_info = pcl_pool.pair_info();
    // Direct set up migration pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            None,
            Some(pcl_pool.address.to_string()),
        )
        .unwrap();

    // try to withdraw from target pool and provide to migration pool in the same transaction
    shared_multisig
        .withdraw(
            &manager2,
            None,
            Some(ProvideParams {
                slippage_tolerance: None,
                auto_stake: None,
            }),
        )
        .unwrap();

    // Check the holder's balance for denom1
    let denom1_before = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom1.clone());

    // Check the holder's balance for denom2
    let denom1_before = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::zero());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&pcl_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(99_998_000));

    // try to update config from manager1
    shared_multisig
        .complete_target_pool_migration(&manager2)
        .unwrap();

    // check if migration is successful
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.migration_pool, None);
    assert_eq!(res.target_pool, Some(pcl_pool.address));
}

#[test]
fn test_withdraw_raqe_quit() {
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);
    let owner = Addr::unchecked(OWNER);
    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &owner,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let factory = MockFactoryBuilder::new(&router).instantiate();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let xyk_pool = factory.instantiate_xyk_pair(&[
        AssetInfo::NativeToken {
            denom: denom1.clone(),
        },
        AssetInfo::NativeToken {
            denom: denom2.clone(),
        },
    ]);

    let xyk_pair_info = xyk_pool.pair_info().unwrap();
    assert_eq!(
        xyk_pair_info.asset_infos,
        vec![
            AssetInfo::NativeToken {
                denom: denom1.clone(),
            },
            AssetInfo::NativeToken {
                denom: denom2.clone(),
            },
        ]
    );

    let shared_multisig = MockSharedMultisigBuilder::new(&router).instantiate(
        &factory.address,
        None,
        Some(xyk_pool.address.to_string()),
    );

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&owner, None, None).unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(xyk_pool.address.clone()));

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // Check the holder's balance for denom1
    let denom1_before = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom1.clone());

    // Check the holder's balance for denom2
    let denom1_before = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(denom1_before.amount, Uint128::new(800_000_000));
    assert_eq!(denom1_before.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(99999000));

    // try to update config from manager1
    shared_multisig.start_rage_quit(&manager2).unwrap();

    // check if rage quit has already started
    let res = shared_multisig.query_config().unwrap();
    assert_eq!(res.rage_quit_started, true);

    // try to provide from manager1
    let err = shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );

    // try to update config from manager1
    let err = shared_multisig
        .complete_target_pool_migration(&manager2)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );

    // try to withdraw from target pool and provide to migration pool in the same transaction
    let err = shared_multisig
        .withdraw(
            &manager2,
            None,
            Some(ProvideParams {
                slippage_tolerance: None,
                auto_stake: None,
            }),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is unavailable. Rage quit has already started"
    );
}

#[test]
fn test_autostake_and_withdraw() {
    let astroport = astroport_address();
    let manager1 = Addr::unchecked(MANAGER1);
    let manager2 = Addr::unchecked(MANAGER2);

    let denom1 = String::from("untrn");
    let denom2 = String::from("ibc/astro");
    let denom3 = String::from("usdt");

    let router = Rc::new(RefCell::new(mock_app(
        &astroport,
        Some(vec![
            Coin {
                denom: denom1.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom2.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: denom3,
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    )));

    let mut generator = MockGeneratorBuilder::new(&router).instantiate();
    let factory = generator.factory();
    let astro_token = generator.astro_token_info();
    let coin_registry = factory.coin_registry();
    coin_registry.add(vec![(denom1.to_owned(), 6), (denom2.to_owned(), 6)]);

    let xyk = factory.instantiate_xyk_pair(&[
        AssetInfo::NativeToken {
            denom: denom1.clone(),
        },
        AssetInfo::NativeToken {
            denom: denom2.clone(),
        },
    ]);

    let xyk_pair_info = xyk.pair_info().unwrap();
    let shared_multisig = MockSharedMultisigBuilder::new(&router).instantiate(
        &factory.address,
        Some(generator.address.clone()),
        None,
    );

    // Direct set up target pool without proposal
    shared_multisig
        .setup_pools(
            &shared_multisig.address,
            Some(xyk.address.to_string()),
            None,
        )
        .unwrap();

    let config = shared_multisig.query_config().unwrap();
    assert_eq!(config.target_pool, Some(xyk.address.clone()));

    // Sends tokens to the multisig
    shared_multisig.send_tokens(&astroport, None, None).unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(900_000_000));
    assert_eq!(res.denom, denom2.clone());

    // try to provide from manager1
    shared_multisig
        .provide(&manager1, PoolType::Target, None, None, None, None)
        .unwrap();

    // try to provide from manager2
    shared_multisig
        .provide(
            &manager2,
            PoolType::Target,
            None,
            Some(Decimal::from_ratio(5u128, 10u128)),
            None,
            None,
        )
        .unwrap();

    // Check the holder's balance for denom1
    let res = shared_multisig.query_native_balance(None, &denom1).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom1.clone());

    // Check the holder's balance for denom2
    let res = shared_multisig.query_native_balance(None, &denom2).unwrap();
    assert_eq!(res.amount, Uint128::new(700_000_000));
    assert_eq!(res.denom, denom2.clone());

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(199999000));

    // Try to unstake from generator
    let err = shared_multisig
        .withdraw_generator(&manager2, Some(Uint128::new(10)))
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Insufficient balance for: contract8. Available balance: 0"
    );

    // try to provide from manager2
    shared_multisig
        .provide(&manager2, PoolType::Target, None, None, Some(true), None)
        .unwrap();

    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::new(100_000_000),
    );

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Default::default(),
            pending_on_proxy: None
        },
    );

    generator.setup_pools(&[(xyk.lp_token().address.to_string(), Uint128::one())]);

    router.borrow_mut().update_block(|b| {
        b.height += 100;
    });

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Uint128::new(100_000_000),
            pending_on_proxy: None
        },
    );

    // try to claim from manager2
    shared_multisig.claim_generator_rewards(&manager2).unwrap();

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Uint128::zero(),
            pending_on_proxy: None
        },
    );

    // Check the holder's ASTRO balance
    let res = shared_multisig
        .query_cw20_balance(&Addr::unchecked(astro_token.to_string()), None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(100_000_000));

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::new(100_000_000),
    );

    // Try to unstake from generator
    shared_multisig.withdraw_generator(&manager2, None).unwrap();

    // Check the holder's LP balance
    let res = shared_multisig
        .query_cw20_balance(&xyk_pair_info.liquidity_token, None)
        .unwrap();
    assert_eq!(res.balance, Uint128::new(299_999_000));

    router.borrow_mut().update_block(|b| {
        b.height += 100;
    });

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::zero(),
    );

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Uint128::zero(),
            pending_on_proxy: None
        },
    );

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::zero(),
    );

    // Try to deposit to generator
    shared_multisig
        .deposit_generator(&manager2, Some(Uint128::new(10)))
        .unwrap();

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::new(10),
    );

    // Try to deposit zero LP tokens to generator
    let err = shared_multisig
        .deposit_generator(&manager2, Some(Uint128::zero()))
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Invalid zero amount");

    // Try to deposit more LP tokens to generator then we have
    let err = shared_multisig
        .deposit_generator(&manager2, Some(Uint128::new(1000000000000)))
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Insufficient balance for: contract8. Available balance: 299998990"
    );

    // Try to deposit all LP tokens to generator
    shared_multisig.deposit_generator(&manager2, None).unwrap();

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::new(299999000),
    );

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Uint128::zero(),
            pending_on_proxy: None
        },
    );

    router.borrow_mut().update_block(|b| {
        b.height += 100;
    });

    assert_eq!(
        generator.pending_token(&xyk.lp_token().address, &shared_multisig.address),
        PendingTokenResponse {
            pending: Uint128::new(99_999_999),
            pending_on_proxy: None
        },
    );

    // Try to unstake from generator
    shared_multisig.withdraw_generator(&manager2, None).unwrap();

    // check the holder's deposit
    assert_eq!(
        generator.query_deposit(&xyk.lp_token(), &shared_multisig.address),
        Uint128::zero(),
    );
}
