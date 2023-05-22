use cosmwasm_std::{Addr, BankMsg, Coin, Uint128};
use cw3::{ProposalResponse, Status, Vote, VoteInfo, VoteResponse};
use cw_multi_test::{App, ContractWrapper, Executor};
use cw_utils::{Duration, ThresholdResponse};

use astroport::shared_multisig::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};

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

fn store_shared_multisig_code(app: &mut App) -> u64 {
    let contract = Box::new(ContractWrapper::new_with_empty(
        astroport_shared_multisig::contract::execute,
        astroport_shared_multisig::contract::instantiate,
        astroport_shared_multisig::contract::query,
    ));

    app.store_code(contract)
}

const OWNER: &str = "owner";
const DAO: &str = "dao";
const MANAGER: &str = "manager";
const CHEATER: &str = "cheater";

#[test]
fn proper_initialization() {
    let owner = Addr::unchecked("owner");
    let manager = Addr::unchecked("manager");
    let dao = Addr::unchecked("dao");
    let mut app = mock_app(&owner, None);

    let shared_multisig_code_id = store_shared_multisig_code(&mut app);

    let shared_multisig_instance = app
        .instantiate_contract(
            shared_multisig_code_id,
            owner,
            &InstantiateMsg {
                max_voting_period: Duration::Height(3),
                dao: dao.to_string(),
                manager: manager.to_string(),
            },
            &[],
            "Astroport shared multisig",
            None,
        )
        .unwrap();

    let config_res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&shared_multisig_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(manager, config_res.manager);
    assert_eq!(dao, config_res.dao);
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
fn check_update_manager() {
    let owner = Addr::unchecked("owner");
    let manager = Addr::unchecked("manager");
    let dao = Addr::unchecked("dao");
    let new_manager = Addr::unchecked("new_manager");
    let recipient = Addr::unchecked("recipient");
    let mut app = mock_app(&owner, None);

    let shared_multisig_code_id = store_shared_multisig_code(&mut app);

    let shared_multisig_instance = app
        .instantiate_contract(
            shared_multisig_code_id,
            owner,
            &InstantiateMsg {
                max_voting_period: Duration::Height(3),
                dao: dao.to_string(),
                manager: manager.to_string(),
            },
            &[],
            "Astroport shared multisig",
            None,
        )
        .unwrap();

    // New manager
    let msg = ExecuteMsg::ProposeNewManager {
        manager: new_manager.to_string(),
        expires_in: 100, // seconds
    };

    let err = app
        .execute_contract(dao.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            new_manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimManager {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    let propose_msg = ExecuteMsg::Propose {
        title: "Transfer 100 tokens".to_string(),
        description: "Need to transfer tokens".to_string(),
        msgs: vec![BankMsg::Send {
            to_address: recipient.to_string(),
            amount: vec![Coin {
                denom: "utrn".to_string(),
                amount: Uint128::new(100_000_000),
            }],
        }
        .into()],
        latest: None,
    };

    // try to propose from manager
    app.execute_contract(
        manager.clone(),
        shared_multisig_instance.clone(),
        &propose_msg,
        &[],
    )
    .unwrap();

    // Try to propose new manager
    app.execute_contract(manager.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap();

    // Claim from DAO
    let err = app
        .execute_contract(
            dao.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimManager {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop manager proposal
    let err = app
        .execute_contract(
            new_manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::DropManagerProposal {},
            &[],
        )
        .unwrap_err();
    // new_manager is not an manager yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        manager.clone(),
        shared_multisig_instance.clone(),
        &ExecuteMsg::DropManagerProposal {},
        &[],
    )
    .unwrap();

    // Try to claim manager
    let err = app
        .execute_contract(
            new_manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimManager {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new manager again
    app.execute_contract(manager.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap();

    // Claim manager
    app.execute_contract(
        new_manager.clone(),
        shared_multisig_instance.clone(),
        &ExecuteMsg::ClaimManager {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&shared_multisig_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.manager, new_manager);
    assert_eq!(res.dao, dao);
}

#[test]
fn check_update_dao() {
    let owner = Addr::unchecked("owner");
    let manager = Addr::unchecked("manager");
    let dao = Addr::unchecked("dao");
    let new_dao = Addr::unchecked("new_dao");
    let recipient = Addr::unchecked("recipient");
    let mut app = mock_app(&owner, None);

    let shared_multisig_code_id = store_shared_multisig_code(&mut app);

    let shared_multisig_instance = app
        .instantiate_contract(
            shared_multisig_code_id,
            owner.clone(),
            &InstantiateMsg {
                max_voting_period: Duration::Height(3),
                dao: dao.to_string(),
                manager: manager.to_string(),
            },
            &[],
            "Astroport shared multisig",
            None,
        )
        .unwrap();

    // New DAO
    let msg = ExecuteMsg::ProposeNewDao {
        dao: new_dao.to_string(),
        expires_in: 100, // seconds
    };

    let err = app
        .execute_contract(manager.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            new_dao.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimDao {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    let propose_msg = ExecuteMsg::Propose {
        title: "Transfer 100 tokens".to_string(),
        description: "Need to transfer tokens".to_string(),
        msgs: vec![BankMsg::Send {
            to_address: recipient.to_string(),
            amount: vec![Coin {
                denom: "utrn".to_string(),
                amount: Uint128::new(100_000_000),
            }],
        }
        .into()],
        latest: None,
    };

    // try to propose from DAO
    app.execute_contract(
        dao.clone(),
        shared_multisig_instance.clone(),
        &propose_msg,
        &[],
    )
    .unwrap();

    // Try to propose new DAO
    app.execute_contract(dao.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap();

    // Claim from manager
    let err = app
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimDao {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop DAO proposal
    let err = app
        .execute_contract(
            new_dao.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::DropDaoProposal {},
            &[],
        )
        .unwrap_err();

    // new_dao is not an DAO yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        dao.clone(),
        shared_multisig_instance.clone(),
        &ExecuteMsg::DropDaoProposal {},
        &[],
    )
    .unwrap();

    // Try to claim DAO
    let err = app
        .execute_contract(
            new_dao.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::ClaimDao {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new DAO again
    app.execute_contract(dao.clone(), shared_multisig_instance.clone(), &msg, &[])
        .unwrap();

    // Claim DAO
    app.execute_contract(
        new_dao.clone(),
        shared_multisig_instance.clone(),
        &ExecuteMsg::ClaimDao {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&shared_multisig_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.manager, manager);
    assert_eq!(res.dao, new_dao);
}

#[test]
fn shared_multisig_controls() {
    let dao = Addr::unchecked(DAO);
    let manager = Addr::unchecked(MANAGER);
    let owner = Addr::unchecked(OWNER);
    let cheater = Addr::unchecked(CHEATER);
    let recipient = Addr::unchecked("recipient");

    let mut router = mock_app(
        &owner,
        Some(vec![Coin {
            denom: "utrn".to_string(),
            amount: Uint128::new(100_000_000_000u128),
        }]),
    );

    let shared_multisig_code_id = store_shared_multisig_code(&mut router);

    let instantiate_msg = InstantiateMsg {
        max_voting_period: Duration::Height(3),
        dao: DAO.to_string(),
        manager: MANAGER.to_string(),
    };

    let shared_multisig_instance = router
        .instantiate_contract(
            shared_multisig_code_id,
            owner.clone(),
            &instantiate_msg,
            &[],
            "Astroport shared multisig",
            None,
        )
        .unwrap();

    // Sends tokens to the multisig
    router
        .send_tokens(
            owner.clone(),
            shared_multisig_instance.clone(),
            &[Coin {
                denom: "utrn".to_string(),
                amount: Uint128::new(200_000_000u128),
            }],
        )
        .unwrap();

    // Check the recipient's balance
    let res = router
        .wrap()
        .query_balance(recipient.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, "utrn");

    // Check the holder's balance
    let res = router
        .wrap()
        .query_balance(shared_multisig_instance.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::new(200_000_000));
    assert_eq!(res.denom, "utrn");

    let transfer_msg = BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![Coin {
            denom: "utrn".to_string(),
            amount: Uint128::new(100_000_000),
        }],
    };

    let propose_msg = ExecuteMsg::Propose {
        title: "Transfer 100 tokens".to_string(),
        description: "Need to transfer tokens".to_string(),
        msgs: vec![transfer_msg.into()],
        latest: None,
    };

    // try to propose from cheater
    let err = router
        .execute_contract(
            cheater.clone(),
            shared_multisig_instance.clone(),
            &propose_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // try to propose from DAO
    router
        .execute_contract(
            dao.clone(),
            shared_multisig_instance.clone(),
            &propose_msg,
            &[],
        )
        .unwrap();

    // Try to vote from cheater
    let err = router
        .execute_contract(
            cheater.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Vote {
                proposal_id: 1,
                vote: Vote::Yes,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // Try to execute with only 1 vote
    let err = router
        .execute_contract(
            dao.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Execute { proposal_id: 1 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // Check DAO vote
    let res: VoteResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Vote {
                proposal_id: 1,
                voter: dao.to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        res,
        VoteResponse {
            vote: Some(VoteInfo {
                proposal_id: 1,
                voter: dao.to_string(),
                vote: Vote::Yes,
                weight: 1
            }),
        }
    );

    // Check Manager vote
    let res: VoteResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Vote {
                proposal_id: 1,
                voter: manager.to_string(),
            },
        )
        .unwrap();
    assert_eq!(res.vote, None);

    // Try to vote from Manager
    router
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Vote {
                proposal_id: 1,
                vote: Vote::No,
            },
            &[],
        )
        .unwrap();

    // Check Manager vote
    let res: VoteResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Vote {
                proposal_id: 1,
                voter: manager.to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        res,
        VoteResponse {
            vote: Some(VoteInfo {
                proposal_id: 1,
                voter: manager.to_string(),
                vote: Vote::No,
                weight: 1
            })
        }
    );

    let err = router
        .execute_contract(
            cheater.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Execute { proposal_id: 1 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // Try to vote from Manager
    let err = router
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Vote {
                proposal_id: 1,
                vote: Vote::Yes,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Already voted on this proposal",
        err.root_cause().to_string()
    );

    // Check the recipient's balance
    let res = router
        .wrap()
        .query_balance(recipient.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::zero());
    assert_eq!(res.denom, "utrn");

    // Check the holder's balance
    let res = router
        .wrap()
        .query_balance(shared_multisig_instance.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::new(200_000_000));
    assert_eq!(res.denom, "utrn");

    // try to propose from DAO
    router
        .execute_contract(
            dao.clone(),
            shared_multisig_instance.clone(),
            &propose_msg,
            &[],
        )
        .unwrap();

    router.update_block(|b| b.height += 4);

    // Try to vote from Manager
    let err = router
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Vote {
                proposal_id: 2,
                vote: Vote::Yes,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Proposal voting period has expired",
        err.root_cause().to_string()
    );

    let err = router
        .execute_contract(
            cheater.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Execute { proposal_id: 2 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Proposal must have passed and not yet been executed",
        err.root_cause().to_string()
    );

    // Check votes status
    let res: VoteResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Vote {
                proposal_id: 1,
                voter: manager.to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        res,
        VoteResponse {
            vote: Some(VoteInfo {
                proposal_id: 1,
                voter: manager.to_string(),
                vote: Vote::No,
                weight: 1
            })
        }
    );

    let res: ProposalResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();
    assert_eq!(res.status, Status::Rejected);

    let res: ProposalResponse = router
        .wrap()
        .query_wasm_smart(
            &shared_multisig_instance,
            &QueryMsg::Proposal { proposal_id: 2 },
        )
        .unwrap();
    assert_eq!(res.status, Status::Rejected);

    // Try to update config from Manager
    let err = router
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                max_voting_period: Duration::Height(10),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // Try to update config from multisig contract
    router
        .execute_contract(
            shared_multisig_instance.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                max_voting_period: Duration::Height(10),
            },
            &[],
        )
        .unwrap();

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&shared_multisig_instance, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(res.max_voting_period, Duration::Height(10));

    // try to propose from DAO
    router
        .execute_contract(
            dao.clone(),
            shared_multisig_instance.clone(),
            &propose_msg,
            &[],
        )
        .unwrap();

    // Try to vote from Manager
    router
        .execute_contract(
            manager.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Vote {
                proposal_id: 3,
                vote: Vote::Yes,
            },
            &[],
        )
        .unwrap();

    // Try to execute with only 1 vote
    router
        .execute_contract(
            recipient.clone(),
            shared_multisig_instance.clone(),
            &ExecuteMsg::Execute { proposal_id: 3 },
            &[],
        )
        .unwrap();

    // Check the recipient's balance
    let res = router
        .wrap()
        .query_balance(recipient.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, "utrn");

    // Check the holder's balance
    let res = router
        .wrap()
        .query_balance(shared_multisig_instance.to_string(), "utrn")
        .unwrap();
    assert_eq!(res.amount, Uint128::new(100_000_000));
    assert_eq!(res.denom, "utrn");
}
