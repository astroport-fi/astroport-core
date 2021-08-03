use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, coins, from_binary, to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, Response,
    StdError, Timestamp, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::balances::WEEK;
use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, ProposalState, QueryMsg, StateResponse,
};
use crate::state::{Config, ExecuteData, Proposal, State, CONFIG, GOVERNANCE_SATE};

const VOTING_TOKEN: &str = "voting_token";
const GUARDIAN: &str = "guardian";
const ADMIN: &str = "admin";
const NEW_ADMIN: &str = "new_admin";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 10000u64;
const DEFAULT_VOTING_DELAY_PERIOD: u64 = 10u64;
const DEFAULT_TIMELOCK_PERIOD: u64 = 2 * 86400u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 100_000_000_000u128;
const DEFAULT_DEPOSIT: u128 = 110_000_000_000u128;

fn mock_init(deps: DepsMut) {
    let msg = InstantiateMsg {
        token: Addr::unchecked(VOTING_TOKEN),
        guardian: Addr::unchecked(GUARDIAN),
        admin: Addr::unchecked(ADMIN),
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        voting_delay_period: DEFAULT_VOTING_DELAY_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_weight: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
    };

    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let _res = instantiate(deps, env, info, msg).expect("contract successfully handles InitMsg");
}

fn mock_env_height(height: u64, time: u64) -> Env {
    let mut env = mock_env();
    env.block.height = height;
    env.block.time = Timestamp::from_seconds(time);
    env
}

fn init_msg() -> InstantiateMsg {
    InstantiateMsg {
        token: Addr::unchecked(VOTING_TOKEN),
        guardian: Addr::unchecked(GUARDIAN),
        admin: Addr::unchecked(ADMIN),
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        voting_delay_period: DEFAULT_VOTING_DELAY_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        proposal_weight: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
    }
}

fn create_propose_msg(
    title: String,
    description: String,
    link: Option<String>,
    execute_msg: Option<Vec<ExecuteData>>,
) -> ExecuteMsg {
    let msg = ExecuteMsg::Propose {
        title,
        description,
        link,
        execute_data: execute_msg,
    };
    msg
}

fn get_locked_balance(deps: Deps, env: Env, addr: Addr) -> Uint128 {
    let balance_msg = QueryMsg::GetBalanceOf { user: addr };
    let balance_res = query(deps, env.clone(), balance_msg).unwrap();

    let voting_power: Uint128 = from_binary(&balance_res).unwrap();
    voting_power
}

fn assert_stake_tokens_result(
    total_share: u128,
    total_deposit: u128,
    new_share: u128,
    proposal_count: u64,
    handle_res: Response,
    deps: DepsMut,
) {
    assert_eq!(
        handle_res.attributes.get(2).expect("no attr"),
        &attr("amount", new_share.to_string())
    );

    let state: State = GOVERNANCE_SATE.load(deps.storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count,
            supply: Uint128(total_share + total_deposit),
            epoch: 0,
            point_history: vec![],
        }
    );
}

fn assert_create_proposal_result(
    proposal_id: u64,
    end_height: u64,
    creator: &str,
    handle_res: Response,
    deps: DepsMut,
) {
    assert_eq!(
        handle_res.attributes,
        vec![
            attr("Action", "ProposalCreated"),
            attr("id", proposal_id.to_string()),
            attr("proposer", creator),
            attr("endBlock", end_height.to_string()),
        ]
    );

    //confirm poll count
    let state: State = GOVERNANCE_SATE.load(deps.storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count: 1,
            supply: Uint128(DEFAULT_DEPOSIT),
            epoch: 0,
            point_history: vec![],
        }
    );
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let msg = init_msg();
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &coins(2, VOTING_TOKEN));
    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config: Config = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        config,
        Config {
            guardian: Addr::unchecked(GUARDIAN),
            admin: Addr::unchecked(ADMIN),
            quorum: Decimal::percent(DEFAULT_QUORUM),
            threshold: Decimal::percent(DEFAULT_THRESHOLD),
            xtrs_token: Addr::unchecked(VOTING_TOKEN),
            voting_period: DEFAULT_VOTING_PERIOD,
            voting_delay_period: DEFAULT_VOTING_DELAY_PERIOD,
            timelock_period: DEFAULT_TIMELOCK_PERIOD,
            expiration_period: DEFAULT_EXPIRATION_PERIOD,
            pending_admin: Addr::unchecked(ADMIN),
            proposal_weight: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
        }
    );
    let state: State = GOVERNANCE_SATE.load(&mut deps.storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count: 0,
            supply: Uint128::zero(),
            epoch: 0,
            point_history: vec![],
        }
    );
}

#[test]
fn proposal_not_found() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    mock_init(deps.as_mut());

    let res = query(deps.as_ref(), env, QueryMsg::GetState { proposal_id: 1 });

    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "state invalid proposal id"),
        Err(e) => panic!("Unexpected error: {:?}", e),
        _ => panic!("Must return error"),
    }
}

#[test]
fn fails_init_invalid_quorum() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let info = mock_info("voter", &coins(11, VOTING_TOKEN));
    let msg = InstantiateMsg {
        token: Addr::unchecked(VOTING_TOKEN),
        guardian: Addr::unchecked(GUARDIAN),
        quorum: Decimal::percent(101),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        voting_delay_period: 0,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_weight: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        admin: Addr::unchecked(ADMIN),
    };

    let res = instantiate(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "quorum must be 0 to 1"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_init_invalid_threshold() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let info = mock_info("voter", &coins(11, VOTING_TOKEN));
    let msg = InstantiateMsg {
        token: Addr::unchecked(VOTING_TOKEN),
        guardian: Addr::unchecked(GUARDIAN),
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(101),
        voting_period: DEFAULT_VOTING_PERIOD,
        voting_delay_period: 0,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_weight: Uint128(DEFAULT_PROPOSAL_DEPOSIT),
        expiration_period: DEFAULT_EXPIRATION_PERIOD,
        admin: Addr::unchecked(ADMIN),
    };

    let res = instantiate(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => {
            assert_eq!(msg, "threshold must be 0 to 1")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_create_proposal_invalid_title() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &vec![]);

    let msg = create_propose_msg("a".to_string(), "test".to_string(), None, None);

    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Title too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_propose_msg(
        "0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234".to_string(),
        "test".to_string(),
        None,
        None,
    );

    match execute(deps.as_mut(), env.clone(), info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Title too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_proposal_invalid_description() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let msg = create_propose_msg("test".to_string(), "a".to_string(), None, None);
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &vec![]);
    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Description too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_propose_msg(
        "test".to_string(),
        "012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678900123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789001234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012341234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456".to_string(),
        None,
        None,
    );

    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Description too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_crete_proposal_invalid_link() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        Some("http://hih".to_string()),
        None,
    );
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &vec![]);
    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Link too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        Some("0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234".to_string()),
        None,
    );

    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "Link too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_proposal_invalid_deposit() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let msg = create_propose_msg("TESTTEST".to_string(), "TESTTEST".to_string(), None, None);
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &vec![]);
    match execute(deps.as_mut(), env.clone(), info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(
            msg,
            format!(
                "Must proposal weight more than {} token",
                DEFAULT_PROPOSAL_DEPOSIT
            )
        ),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn create_proposal() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_CREATOR, &vec![]);

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg("test".to_string(), "test".to_string(), None, None);
    let handle_res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );
}

#[test]
fn query_proposals() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env_height(0, 10000);
    let mut info = mock_info(VOTING_TOKEN, &vec![]);

    info.sender = Addr::unchecked(TEST_CREATOR);
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    info.sender = Addr::unchecked(TEST_VOTER);
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    info.sender = Addr::unchecked(TEST_CREATOR);
    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        Some("http://google.com".to_string()),
        None,
    );

    let _handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    info.sender = Addr::unchecked(TEST_VOTER);
    let msg = create_propose_msg("test2".to_string(), "test2".to_string(), None, None);
    let _handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();

    let mut res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetProposal { proposal_id: 1 },
    )
    .unwrap();
    let response: Proposal = from_binary(&res).unwrap();
    assert_eq!(
        response,
        Proposal {
            id: 1u64,
            proposer: Addr::unchecked(TEST_CREATOR),
            eta: 0,
            title: "test".to_string(),
            description: "test".to_string(),
            link: Some("http://google.com".to_string()),
            execute_data: None,
            start_block: DEFAULT_VOTING_DELAY_PERIOD,
            end_block: DEFAULT_VOTING_DELAY_PERIOD + DEFAULT_VOTING_PERIOD,
            for_votes: Uint128::zero(),
            against_votes: Uint128::zero(),
            canceled: false,
            executed: false,
        }
    );
    res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetProposal { proposal_id: 2 },
    )
    .unwrap();
    let response: Proposal = from_binary(&res).unwrap();
    assert_eq!(
        response,
        Proposal {
            id: 2u64,
            proposer: Addr::unchecked(TEST_VOTER),
            eta: 0,
            title: "test2".to_string(),
            description: "test2".to_string(),
            link: None,
            execute_data: None,
            start_block: DEFAULT_VOTING_DELAY_PERIOD,
            end_block: DEFAULT_VOTING_DELAY_PERIOD + DEFAULT_VOTING_PERIOD,
            for_votes: Uint128::zero(),
            against_votes: Uint128::zero(),
            canceled: false,
            executed: false,
        },
    );
}

#[test]
fn create_proposal_no_quorum() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_CREATOR, &vec![]);

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg("test".to_string(), "test".to_string(), None, None);

    let handle_res = execute(deps.as_mut(), env, info, msg.clone()).unwrap();
    assert_create_proposal_result(
        1,
        DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );
}

#[test]
fn fails_end_lock_before_end_height() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_CREATOR, &vec![]);

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time,
    };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg);
    match handle_res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => {
            assert_eq!(msg, "Can only lock until time in the future")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn end_proposal() {
    const PROPOSAL_START_HEIGHT: u64 = 1000;
    const PROPOSAL_ID: u64 = 1;
    let stake_amount = DEFAULT_DEPOSIT;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(PROPOSAL_START_HEIGHT, 10000);
    let creator_info = mock_info(TEST_CREATOR, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20ExecuteMsg::Burn { amount: Uint128(1) }).unwrap();

    //add three messages with different order
    let mut execute_msgs: Vec<ExecuteData> = vec![];

    execute_msgs.push(ExecuteData {
        order: 3u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: exec_msg_bz3.clone(),
    });

    execute_msgs.push(ExecuteData {
        order: 2u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: exec_msg_bz2.clone(),
    });

    execute_msgs.push(ExecuteData {
        order: 1u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), creator_info.clone(), lock_msg).unwrap();

    let handle_res = execute(deps.as_mut(), env.clone(), creator_info.clone(), msg).unwrap();

    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );

    env.block.height += DEFAULT_VOTING_DELAY_PERIOD + 1;

    let msg = ExecuteMsg::CreateLock {
        amount: Uint128(stake_amount as u128),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    //let _res = execute(deps.as_mut(), creator_env.clone(), creator_info.clone(), lock_msg).unwrap();
    let mut info = mock_info(TEST_VOTER, &[]);
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_DEPOSIT,
        stake_amount,
        1,
        handle_res,
        deps.as_mut(),
    );

    let vote_msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: true,
    };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), vote_msg.clone()).unwrap();
    let power = get_locked_balance(deps.as_ref(), env.clone(), Addr::unchecked(TEST_VOTER));
    assert_eq!(
        handle_res.attributes,
        vec![
            attr("Action", "VoteCast"),
            attr("proposal_id", PROPOSAL_ID),
            attr("vote_power", power.to_string()),
            attr("voter", TEST_VOTER),
            attr("support", "true"),
        ]
    );

    // not in passed status
    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    match handle_res {
        ContractError::ProposalError { msg, .. } => assert_eq!(
            msg,
            "Governor queue proposal can only be queued if it is succeeded"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }

    // not in passed status
    let msg = ExecuteMsg::Execute { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    match handle_res {
        ContractError::ProposalError { msg, .. } => {
            assert_eq!(msg, "queue proposal can only be queued if it is succeeded")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    info.sender = Addr::unchecked(TEST_CREATOR);
    // let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), vote_msg.clone()).unwrap();
    // let power = get_locked_balance(deps.as_ref(),env.clone(), Addr::unchecked(TEST_CREATOR));
    // assert_eq!(
    //     handle_res.attributes,
    //     vec![
    //         attr("Action", "VoteCast"),
    //         attr("proposal_id", PROPOSAL_ID),
    //         attr("vote_power", power.to_string()),
    //         attr("voter", TEST_CREATOR),
    //         attr("support", "true"),
    //     ]
    // );
    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    //println!("{:?}", res.state);
    assert_eq!(ProposalState::Active, res.state);

    env.block.height += DEFAULT_VOTING_PERIOD;

    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    //println!("{:?}", res.state);
    assert_eq!(ProposalState::Succeeded, res.state);

    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_eq!(
        handle_res.attributes,
        vec![
            attr("Action", "ProposalQueued"),
            attr("proposalId", "1"),
            attr("Eta", "183811"),
        ]
    );

    // timelock_period has not expired
    let handle_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Execute { proposal_id: 1 },
    )
    .unwrap_err();

    match handle_res {
        ContractError::ProposalError { msg, .. } => {
            assert_eq!(msg, "Timelock period has not expired")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    env.block.height += DEFAULT_TIMELOCK_PERIOD;
    let handle_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Execute { proposal_id: 1 },
    )
    .unwrap();
    assert_eq!(
        handle_res.attributes,
        vec![attr("Action", "ProposalExecute"), attr("proposalId", "1"),]
    );
    assert_eq!(
        handle_res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz.clone(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz2,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz3,
                send: vec![],
            })
        ]
    );
}

#[test]
fn expire_proposal() {
    const PROPOSAL_START_HEIGHT: u64 = 1000;
    const PROPOSAL_ID: u64 = 1;
    let stake_amount = DEFAULT_DEPOSIT;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(PROPOSAL_START_HEIGHT, 10000);
    let mut info = mock_info(TEST_CREATOR, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128(123),
    })
    .unwrap();
    let mut execute_msgs: Vec<ExecuteData> = vec![];
    execute_msgs.push(ExecuteData {
        order: 1u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: exec_msg_bz.clone(),
    });

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );

    env.block.height += DEFAULT_VOTING_DELAY_PERIOD + 1;

    info.sender = Addr::unchecked(TEST_VOTER);
    let msg = ExecuteMsg::CreateLock {
        amount: Uint128(stake_amount as u128),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    let msg = ExecuteMsg::Vote {
        proposal_id: PROPOSAL_ID,
        support: true,
    };

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_eq!(
        handle_res.attributes,
        vec![
            attr("Action", "VoteCast"),
            attr("proposal_id", PROPOSAL_ID),
            attr("vote_power", "109678764800"),
            attr("voter", TEST_VOTER),
            attr("support", "true"),
        ]
    );

    // Poll is not in passed status
    //env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match handle_res {
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(
            msg,
            "Governor queue proposal can only be queued if it is succeeded"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }

    env.block.height += DEFAULT_VOTING_PERIOD;

    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    //println!("{:?}", res.state);
    assert_eq!(res.state, ProposalState::Succeeded);

    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        handle_res.attributes,
        vec![
            attr("Action", "ProposalQueued"),
            attr("proposalId", "1"),
            attr("Eta", "183811"),
        ]
    );

    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    //println!("{:?}", res.state);
    assert_eq!(res.state, ProposalState::Queued);

    env.block.height += DEFAULT_EXPIRATION_PERIOD + DEFAULT_TIMELOCK_PERIOD;
    // Expiration period has not been passed
    let msg = ExecuteMsg::Execute { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match handle_res {
        Err(ContractError::ProposalError { msg, .. }) => {
            assert_eq!(msg, "queue proposal can only be queued if it is succeeded")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    //println!("{:?}", res.state);
    assert_eq!(res.state, ProposalState::Expired);
}

#[test]
fn end_proposal_zero_quorum() {
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(1000, 10000);
    let mut info = mock_info(TEST_CREATOR, &vec![]);

    let mut execute_msgs: Vec<ExecuteData> = vec![];
    execute_msgs.push(ExecuteData {
        order: 1u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: Uint128(123),
        })
        .unwrap(),
    });

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );

    info.sender = Addr::unchecked(TEST_CREATOR);
    env.block.height += DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD + 1;

    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match handle_res {
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(
            msg,
            "Governor queue proposal can only be queued if it is succeeded"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }
    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    assert_eq!(res.state, ProposalState::Defeated);
}

#[test]
fn end_proposal_quorum_rejected() {
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(1000, 10000);
    let mut info = mock_info(TEST_CREATOR, &vec![]);

    let mut execute_msgs: Vec<ExecuteData> = vec![];
    execute_msgs.push(ExecuteData {
        order: 1u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: Uint128(123),
        })
        .unwrap(),
    });
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );

    info.sender = Addr::unchecked(TEST_VOTER);
    env.block.height += DEFAULT_VOTING_DELAY_PERIOD + 1;

    info.sender = Addr::unchecked(TEST_VOTER);
    let msg = ExecuteMsg::CreateLock {
        amount: Uint128(45_000_000_000u128),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    let msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: true,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    env.block.height += DEFAULT_VOTING_PERIOD;
    info.sender = Addr::unchecked(TEST_CREATOR);
    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match handle_res {
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(
            msg,
            "Governor queue proposal can only be queued if it is succeeded"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }
    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    assert_eq!(res.state, ProposalState::Defeated);
}

#[test]
fn end_proposal_nay_rejected() {
    let voter1_stake = 50_000_000_000u128;
    let voter2_stake = 100_000_000_000u128;
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(1000, 10000);
    let mut info = mock_info(TEST_CREATOR, &vec![]);

    let mut execute_msgs: Vec<ExecuteData> = vec![];
    execute_msgs.push(ExecuteData {
        order: 1u64,
        contract: Addr::unchecked(VOTING_TOKEN),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: Uint128(123),
        })
        .unwrap(),
    });
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let msg = create_propose_msg(
        "test".to_string(),
        "test".to_string(),
        None,
        Some(execute_msgs),
    );

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_create_proposal_result(
        1,
        env.block.height + DEFAULT_VOTING_PERIOD + DEFAULT_VOTING_DELAY_PERIOD,
        TEST_CREATOR,
        handle_res,
        deps.as_mut(),
    );

    info.sender = Addr::unchecked(TEST_VOTER);
    env.block.height += DEFAULT_VOTING_DELAY_PERIOD + 1;

    //voter1
    info.sender = Addr::unchecked(TEST_VOTER);
    let msg = ExecuteMsg::CreateLock {
        amount: Uint128(voter1_stake),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    let msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: true,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    //voter2
    info.sender = Addr::unchecked(TEST_VOTER_2);
    let msg = ExecuteMsg::CreateLock {
        amount: Uint128(voter2_stake),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    let msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: false,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    env.block.height += DEFAULT_VOTING_PERIOD;
    info.sender = Addr::unchecked(TEST_CREATOR);
    let msg = ExecuteMsg::Queue { proposal_id: 1 };
    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match handle_res {
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(
            msg,
            "Governor queue proposal can only be queued if it is succeeded"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }
    let handle_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::GetState { proposal_id: 1 },
    )
    .unwrap();
    let res: StateResponse = from_binary(&handle_res).unwrap();
    assert_eq!(res.state, ProposalState::Defeated);
}

#[test]
fn vote_power() {
    let time_lock = 2 * 365 * 86400;
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());
    let mut env = mock_env_height(0, 10000);
    let info = mock_info(TEST_CREATOR, &coins(2, VOTING_TOKEN));

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(time_lock),
    };
    let lock_time =
        ((env.block.time.plus_seconds(time_lock).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_CREATOR),
            attr("amount", DEFAULT_DEPOSIT.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    let power = get_locked_balance(deps.as_ref(), env.clone(), Addr::unchecked(TEST_CREATOR));
    assert_eq!(power, Uint128(109678764800u128));

    //tor example 1 block i second 1 day = 86400 sec/block

    let block_per_day = 86400;
    let second = 86400;

    let power = vec![
        105158316800u128,
        100637868800u128,
        96117420800u128,
        91596972800u128,
        87076524800u128,
        82556076800u128,
        78035628800u128,
        73515180800u128,
        68994732800u128,
        64474284800u128,
        59953836800u128,
        55433388800u128,
        50912940800u128,
        46392492800u128,
        41872044800u128,
        37351596800u128,
        32831148800u128,
        28310700800u128,
        23790252800u128,
        19269804800u128,
        14749356800u128,
        10228908800u128,
        5708460800u128,
        1188012800u128,
        0u128,
    ];

    for pwr in power {
        env.block.height += block_per_day * 30;
        env.block.time = env.block.time.plus_seconds(second * 30);
        let power = get_locked_balance(deps.as_ref(), env.clone(), Addr::unchecked(TEST_CREATOR));
        assert_eq!(power, Uint128(pwr));
    }
}

#[test]
fn withdraw_voting_tokens() {
    let time_lock = 2 * 365 * 86400;
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());

    let mut env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &coins(2, VOTING_TOKEN));

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(time_lock),
    };
    let lock_time =
        ((env.block.time.plus_seconds(time_lock).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER),
            attr("amount", DEFAULT_DEPOSIT.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    let state: State = GOVERNANCE_SATE.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count: 0,
            supply: Uint128(DEFAULT_DEPOSIT),
            epoch: 0,
            point_history: vec![],
        }
    );

    env.block.time = env.block.time.plus_seconds(time_lock);
    let msg = ExecuteMsg::Withdraw {};

    let handle_res = execute(deps.as_mut(), env, info.clone(), msg.clone()).unwrap();
    let msg = handle_res.messages.get(0).expect("no message");

    assert_eq!(
        msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: Addr::unchecked(VOTING_TOKEN).to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: Addr::unchecked(TEST_VOTER).to_string(),
                amount: Uint128(DEFAULT_DEPOSIT),
            })
            .unwrap(),
            send: vec![],
        })
    );

    let state: State = GOVERNANCE_SATE.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count: 0,
            supply: Uint128::zero(),
            epoch: 0,
            point_history: vec![],
        }
    );
}

#[test]
fn fails_withdraw_voting_tokens_no_stake() {
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());

    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &coins(2, VOTING_TOKEN));

    let msg = ExecuteMsg::Withdraw {};

    let res = execute(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => assert_eq!(msg, "Nothing staked"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_withdraw_locked_tokens() {
    let time_lock = 2 * 365 * 86400;
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(deps.as_mut());

    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &coins(2, VOTING_TOKEN));

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(time_lock),
    };
    let lock_time =
        ((env.block.time.plus_seconds(time_lock).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER),
            attr("amount", DEFAULT_DEPOSIT.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    let state: State = GOVERNANCE_SATE.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        State {
            owner: Addr::unchecked(TEST_CREATOR),
            proposal_count: 0,
            supply: Uint128(DEFAULT_DEPOSIT),
            epoch: 0,
            point_history: vec![],
        }
    );

    //env.block.time = env.block.time.plus_seconds(time_lock);
    let msg = ExecuteMsg::Withdraw {};
    let res = execute(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => {
            assert_eq!(msg, "The lock didn't expire")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_twice() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let mut env = mock_env_height(0, 10000);
    let mut info = mock_info(VOTING_TOKEN, &vec![]);

    info.sender = Addr::unchecked(TEST_CREATOR);
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    info.sender = Addr::unchecked(TEST_VOTER);
    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    info.sender = Addr::unchecked(TEST_CREATOR);
    let msg = create_propose_msg("test".to_string(), "test".to_string(), None, None);
    let _handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    env.block.height += DEFAULT_VOTING_DELAY_PERIOD + 1;
    info.sender = Addr::unchecked(TEST_VOTER);
    let vote_msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: true,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), vote_msg.clone()).unwrap();
    let res = execute(deps.as_mut(), env.clone(), info.clone(), vote_msg.clone());
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => assert_eq!(msg, "voter already voted"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_without_proposal() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());
    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &vec![]);

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(DEFAULT_DEPOSIT),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();

    let vote_msg = ExecuteMsg::Vote {
        proposal_id: 1,
        support: true,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), vote_msg.clone());

    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::ProposalError { msg, .. }) => {
            assert_eq!(msg, "state invalid proposal id")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn stake_voting_tokens() {
    let deposit = 1_000_00u128;
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &vec![]);

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(deposit),
        lock: env.block.time.plus_seconds(2 * 365 * 86400),
    };
    let lock_time =
        ((env.block.time.plus_seconds(2 * 365 * 86400).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER.to_string()),
            attr("amount", deposit.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: Addr::unchecked(VOTING_TOKEN).to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: Addr::unchecked(TEST_VOTER).to_string(),
                recipient: Addr::unchecked(MOCK_CONTRACT_ADDR).to_string(),
                amount: Uint128(deposit),
            })
            .unwrap(),
            send: vec![],
        })]
    );
    assert_stake_tokens_result(0, deposit, deposit, 0, res, deps.as_mut());
}

#[test]
fn fails_staking_wrong_token() {
    //not implemented yet
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    // update owner
    let env = mock_env();
    let mut info = mock_info(ADMIN, &[]);
    let msg = ExecuteMsg::UpdateGovernanceConfig {
        guardian: Some(Addr::unchecked("addr0001")),
        quorum: None,
        threshold: None,
        voting_period: None,
        timelock_period: None,
        expiration_period: None,
        proposal_weight: None,
        voting_delay_period: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.guardian.as_str());
    assert_eq!(Decimal::percent(DEFAULT_QUORUM), config.quorum);
    assert_eq!(Decimal::percent(DEFAULT_THRESHOLD), config.threshold);
    assert_eq!(DEFAULT_VOTING_PERIOD, config.voting_period);
    assert_eq!(DEFAULT_TIMELOCK_PERIOD, config.timelock_period);
    assert_eq!(DEFAULT_PROPOSAL_DEPOSIT, config.proposal_weight.u128());

    // update left items
    //let env = mock_env("addr0001", &[]);
    let msg = ExecuteMsg::UpdateGovernanceConfig {
        guardian: None,
        quorum: Some(Decimal::percent(20)),
        threshold: Some(Decimal::percent(75)),
        voting_period: Some(20000u64),
        timelock_period: Some(20000u64),
        expiration_period: Some(30000u64),
        proposal_weight: Some(Uint128(123u128)),
        voting_delay_period: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.guardian.as_str());
    assert_eq!(Decimal::percent(20), config.quorum);
    assert_eq!(Decimal::percent(75), config.threshold);
    assert_eq!(20000u64, config.voting_period);
    assert_eq!(20000u64, config.timelock_period);
    assert_eq!(30000u64, config.expiration_period);
    assert_eq!(123u128, config.proposal_weight.u128());

    //change ADMIN

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::SetPendingAdmin {
            admin: Addr::unchecked(NEW_ADMIN),
        },
    )
    .unwrap();
    assert_eq!(res.attributes, vec![attr("NewPendingAdmin", NEW_ADMIN),]);
    info.sender = Addr::unchecked(NEW_ADMIN);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::AcceptAdmin {},
    )
    .unwrap();
    assert_eq!(res.attributes, vec![attr("NewAdmin", NEW_ADMIN),]);

    // Unauthorzied err
    info.sender = Addr::unchecked(ADMIN);
    let msg = ExecuteMsg::UpdateGovernanceConfig {
        guardian: None,
        quorum: None,
        threshold: None,
        voting_period: None,
        timelock_period: None,
        expiration_period: None,
        proposal_weight: None,
        voting_delay_period: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Err(ContractError::Unauthorized { .. }) => {}
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn change_amount_stake_voting_tokens() {
    let deposit = 1_000_00u128;
    let locked = 365 * 86400;
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let mut env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &vec![]);

    let msg = ExecuteMsg::IncreaseAmount {
        amount: Uint128(deposit),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => assert_eq!(msg, "No existing lock found"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(deposit),
        lock: env.block.time.plus_seconds(locked),
    };
    let lock_time = ((env.block.time.plus_seconds(locked).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER.to_string()),
            attr("amount", deposit.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: Addr::unchecked(VOTING_TOKEN).to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: Addr::unchecked(TEST_VOTER).to_string(),
                recipient: Addr::unchecked(MOCK_CONTRACT_ADDR).to_string(),
                amount: Uint128(deposit),
            })
            .unwrap(),
            send: vec![],
        })]
    );
    assert_stake_tokens_result(0, deposit, deposit, 0, res, deps.as_mut());

    let msg = ExecuteMsg::IncreaseAmount {
        amount: Uint128::zero(),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => assert_eq!(msg, "Amount to small"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    let msg = ExecuteMsg::IncreaseAmount {
        amount: Uint128(deposit),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER.to_string()),
            attr("amount", deposit.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: Addr::unchecked(VOTING_TOKEN).to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: Addr::unchecked(TEST_VOTER).to_string(),
                recipient: Addr::unchecked(MOCK_CONTRACT_ADDR).to_string(),
                amount: Uint128(deposit),
            })
            .unwrap(),
            send: vec![],
        })]
    );
    assert_stake_tokens_result(deposit, deposit, deposit, 0, res, deps.as_mut());

    let msg = ExecuteMsg::IncreaseAmount {
        amount: Uint128(deposit),
    };
    env.block.time = env.block.time.plus_seconds(locked);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => {
            assert_eq!(msg, "Cannot add to expired lock. Withdraw")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn change_unlock_time_stake_voting_tokens() {
    let deposit = 1_000_00u128;
    let locked = 365 * 86400;
    let mut deps = mock_dependencies(&[]);
    mock_init(deps.as_mut());

    let mut env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &vec![]);

    let msg = ExecuteMsg::IncreaseUnlockTime {
        unlock_time: env.block.time.plus_seconds(locked),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => assert_eq!(msg, "Lock expired"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    let lock_msg = ExecuteMsg::CreateLock {
        amount: Uint128(deposit),
        lock: env.block.time.plus_seconds(locked),
    };
    let lock_time = ((env.block.time.plus_seconds(locked).nanos() / 1_000_000_000) / WEEK) * WEEK;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), lock_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER.to_string()),
            attr("amount", deposit.to_string()),
            attr("end", lock_time.to_string()),
        ]
    );

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: Addr::unchecked(VOTING_TOKEN).to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: Addr::unchecked(TEST_VOTER).to_string(),
                recipient: Addr::unchecked(MOCK_CONTRACT_ADDR).to_string(),
                amount: Uint128(deposit),
            })
            .unwrap(),
            send: vec![],
        })]
    );
    assert_stake_tokens_result(0, deposit, deposit, 0, res, deps.as_mut());

    let msg = ExecuteMsg::IncreaseUnlockTime {
        unlock_time: env.block.time.plus_seconds(locked),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => {
            assert_eq!(msg, "Can only increase lock duration")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    let msg = ExecuteMsg::IncreaseUnlockTime {
        unlock_time: env.block.time.plus_seconds(locked * 3),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => {
            assert_eq!(msg, "Voting lock can be 2 years max")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    let msg = ExecuteMsg::IncreaseUnlockTime {
        unlock_time: env.block.time.plus_seconds(2 * locked),
    };
    let lock_time =
        ((env.block.time.plus_seconds(locked * 2).nanos() / 1_000_000_000) / WEEK) * WEEK;

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
            attr("addr", TEST_VOTER.to_string()),
            attr("amount", "0"),
            attr("end", lock_time.to_string()),
        ]
    );

    assert_eq!(res.messages, vec![]);
    assert_stake_tokens_result(0, deposit, 0, 0, res, deps.as_mut());
    let msg = ExecuteMsg::IncreaseUnlockTime {
        unlock_time: env.block.time.plus_seconds(locked * 2),
    };
    env.block.time = env.block.time.plus_seconds(locked * 3);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceError { msg, .. }) => assert_eq!(msg, "Lock expired"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}
