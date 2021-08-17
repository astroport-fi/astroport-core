use std::ops::Add;

use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, MemoryStorage, MessageInfo,
    OwnedDeps, ReplyOn, Response, StdError, SubMsg, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg};

use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{Config, PoolInfo, UserInfo, CONFIG, POOL_INFO, USER_INFO};
use astroport::gauge::{
    ExecuteMsg, InstantiateMsg, PendingTokenResponse, PoolLengthResponse, QueryMsg,
};

fn _do_instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    dev: Addr,
    astro_toke: Addr,
    tokens_per_block: Uint128,
    start_block: u64,
    bonus_end_block: u64,
) {
    let instantiate_msg = InstantiateMsg {
        token: astro_toke,
        dev_addr: dev,
        tokens_per_block,
        start_block,
        bonus_end_block,
        allowed_reward_proxies: vec![],
    };
    let res = instantiate(deps, _env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

fn get_token_balance(deps: Deps, user: Addr, token: Addr) -> Uint128 {
    let balance: BalanceResponse = deps
        .querier
        .query_wasm_smart(
            token,
            &cw20::Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    balance.balance
}

fn execute_messages_token_contract(
    deps: &mut OwnedDeps<MemoryStorage, MockApi, WasmMockQuerier>,
    res: Response,
) {
    for request in res.messages {
        match request {
            SubMsg {
                msg:
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr,
                        msg,
                        funds: _,
                    }),
                ..
            } => match from_binary(&msg).unwrap() {
                Cw20ExecuteMsg::Transfer { recipient, amount } => {
                    println!(
                        "Transfer contract address: {}, to {}, amount: {}",
                        contract_addr,
                        recipient,
                        amount.to_string()
                    );
                    deps.querier.sub_balance(
                        Addr::unchecked(MOCK_CONTRACT_ADDR),
                        Addr::unchecked(contract_addr.clone()),
                        amount.clone(),
                    );
                    deps.querier.add_balance(
                        Addr::unchecked(recipient),
                        Addr::unchecked(contract_addr),
                        amount,
                    );
                }
                Cw20ExecuteMsg::Mint { recipient, amount } => {
                    println!(
                        "Mint contract address: {}, to {}, amount: {}",
                        contract_addr,
                        recipient,
                        amount.to_string()
                    );
                    deps.querier.add_balance(
                        Addr::unchecked(recipient),
                        Addr::unchecked(contract_addr),
                        amount,
                    );
                }
                Cw20ExecuteMsg::TransferFrom {
                    owner,
                    recipient,
                    amount,
                } => {
                    println!(
                        "TransferFrom contract address: {}, from: {}, to {}, amount: {}",
                        contract_addr,
                        owner,
                        recipient,
                        amount.to_string()
                    );
                    deps.querier.sub_balance(
                        Addr::unchecked(owner),
                        Addr::unchecked(contract_addr.clone()),
                        amount.clone(),
                    );
                    deps.querier.add_balance(
                        Addr::unchecked(recipient),
                        Addr::unchecked(contract_addr),
                        amount,
                    );
                }
                _ => panic!("DO NOT ENTER HERE"),
            },
            _ => {}
        }
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let dev = Addr::unchecked("dev0000");
    let env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let token_amount = Uint128::from(10u128);

    let instantiate_msg = InstantiateMsg {
        token: astro_token_contract,
        dev_addr: dev,
        tokens_per_block: token_amount,
        start_block: 2,
        bonus_end_block: 10,
        allowed_reward_proxies: vec![],
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        config,
        Config {
            owner: Addr::unchecked("addr0000"),
            astro_token: Addr::unchecked("astro-token"),
            dev_addr: Addr::unchecked("dev0000"),
            bonus_end_block: 10,
            tokens_per_block: token_amount,
            total_alloc_point: 0,
            start_block: 2,
            allowed_reward_proxies: vec![],
        }
    )
}

#[test]
fn execute_add() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = Addr::unchecked("addr0000");
    let dev = Addr::unchecked("dev0000");
    let user = Addr::unchecked("addr0001");
    let env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");
    let lp_token_contract1 = Addr::unchecked("lp-token001");
    let token_amount = Uint128::from(10u128);
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        token_amount,
        env.block.height.add(10),
        env.block.height.add(110),
    );

    let res = query(deps.as_ref(), env.clone(), QueryMsg::PoolLength {}).unwrap();
    let pool_length: PoolLengthResponse = from_binary(&res).unwrap();
    assert_eq!(pool_length.length, 0);

    let msg = ExecuteMsg::Add {
        alloc_point: 10,
        token: lp_token_contract.clone(),
        reward_proxy: None,
        with_update: false,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract.clone())
        .unwrap();

    assert_eq!(
        config,
        Config {
            owner: owner.clone(),
            astro_token: Addr::unchecked("astro-token"),
            dev_addr: dev.clone(),
            bonus_end_block: env.block.height.add(110),
            tokens_per_block: Uint128::from(10u128),
            total_alloc_point: 10,
            start_block: env.block.height.add(10),
            allowed_reward_proxies: vec![],
        }
    );
    assert_eq!(
        pool_info,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
            reward_proxy: None,
        }
    );
    let res = query(deps.as_ref(), env.clone(), QueryMsg::PoolLength {}).unwrap();
    let pool_length: PoolLengthResponse = from_binary(&res).unwrap();
    assert_eq!(pool_length.length, 1);

    let msg = ExecuteMsg::Add {
        alloc_point: 20,
        token: lp_token_contract.clone(),
        reward_proxy: None,
        with_update: false,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::TokenPoolAlreadyExists { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    info.sender = user;
    let msg = ExecuteMsg::Add {
        alloc_point: 20,
        token: lp_token_contract1.clone(),
        reward_proxy: None,
        with_update: false,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::Unauthorized { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
    info.sender = owner.clone();
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info1 = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract.clone())
        .unwrap();
    let pool_info2 = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract1.clone())
        .unwrap();

    assert_eq!(
        config,
        Config {
            owner: owner.clone(),
            astro_token: Addr::unchecked("astro-token"),
            dev_addr: dev.clone(),
            bonus_end_block: env.block.height.add(110),
            tokens_per_block: Uint128::from(10u128),
            total_alloc_point: 10 + 20,
            start_block: env.block.height.add(10),
            allowed_reward_proxies: vec![],
        }
    );
    assert_eq!(
        pool_info1,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
            reward_proxy: None,
        }
    );
    assert_eq!(
        pool_info2,
        PoolInfo {
            alloc_point: 20,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
            reward_proxy: None,
        }
    );
    let res = query(deps.as_ref(), env.clone(), QueryMsg::PoolLength {}).unwrap();
    let pool_length: PoolLengthResponse = from_binary(&res).unwrap();
    assert_eq!(pool_length.length, 2);
}

#[test]
fn execute_set() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = Addr::unchecked("addr0000");
    let dev = Addr::unchecked("dev0000");
    let env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    let token_amount = Uint128::from(10u128);
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        token_amount,
        env.block.height.add(10),
        env.block.height.add(110),
    );

    let msg = ExecuteMsg::Add {
        alloc_point: 10,
        token: lp_token_contract.clone(),
        reward_proxy: None,
        with_update: false,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract.clone())
        .unwrap();
    assert_eq!(config.total_alloc_point, 10);
    assert_eq!(
        pool_info,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
            reward_proxy: None,
        }
    );

    info.sender = Addr::unchecked("addr0001");
    let wr = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Set {
            token: lp_token_contract.clone(),
            alloc_point: 20,
            with_update: false,
        },
    );
    match wr {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::Unauthorized { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
    info.sender = owner;
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Set {
            token: lp_token_contract.clone(),
            alloc_point: 20,
            with_update: false,
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract.clone())
        .unwrap();
    assert_eq!(config.total_alloc_point, 20);
    assert_eq!(pool_info.alloc_point, 20);

    let msg = ExecuteMsg::Add {
        alloc_point: 100,
        token: Addr::unchecked("come_token"),
        reward_proxy: None,
        with_update: false,
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Set {
            token: lp_token_contract.clone(),
            alloc_point: 50,
            with_update: false,
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO
        .load(deps.as_ref().storage, &lp_token_contract.clone())
        .unwrap();
    assert_eq!(config.total_alloc_point, 150); //old: token120+token2 100; new: total 120 -20+50 token1
    assert_eq!(pool_info.alloc_point, 50);
}

#[test]
fn execute_deposit() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let user = Addr::unchecked("user0000");
    let dev = Addr::unchecked("dev000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");
    // mock start balances
    deps.querier.set_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(10000u128),
    );
    let token_amount = Uint128::from(100u128);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        token_amount,
        env.block.height,
        env.block.height.add(100),
    );

    let _res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 10,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();

    info.sender = user.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(1000u128),
        },
    )
    .unwrap();
    let transfer_from_msg = res.messages.get(0).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                    amount: Uint128::from(1000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );
    assert_eq!(res.attributes, vec![attr("Action", "Deposit"),],);
    //mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    let user_info = USER_INFO
        .load(
            deps.as_ref().storage,
            (
                &Addr::unchecked("lp-token000"),
                &Addr::unchecked("user0000"),
            ),
        )
        .unwrap();
    assert_eq!(
        user_info,
        UserInfo {
            amount: Uint128::from(1000u128),
            reward_debt: Uint128::zero(),
        }
    );
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(2000u128),
        },
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                    amount: Uint128::from(2000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }],
    );
    //mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    env.block.height = env.block.height.add(50);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(3000u128),
        },
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: dev.to_string(),
                        amount: Uint128::from(5000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                        amount: Uint128::from(50000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: info.sender.to_string(),
                        amount: Uint128::from(49999u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: lp_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                        amount: Uint128::from(3000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            }
        ]
    );
    // mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);
    let user_info = USER_INFO
        .load(
            deps.as_ref().storage,
            (
                &Addr::unchecked("lp-token000"),
                &Addr::unchecked("user0000"),
            ),
        )
        .unwrap();
    assert_eq!(
        user_info,
        UserInfo {
            amount: Uint128::from(6000u128),
            reward_debt: Uint128::from(99999u128),
        }
    );
}

#[test]
fn execute_withdraw() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let user = Addr::unchecked("user0000");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");
    // mock start balances
    deps.querier.set_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(10u128),
        env.block.height,
        env.block.height.add(1000),
    );

    let _res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 10,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();

    info.sender = user.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(100u128),
        },
    )
    .unwrap();
    //mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(50u128),
        },
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(50u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }]
    );
    execute_messages_token_contract(&mut deps, res);

    let user_info = USER_INFO
        .load(
            deps.as_ref().storage,
            (
                &Addr::unchecked("lp-token000"),
                &Addr::unchecked("user0000"),
            ),
        )
        .unwrap();
    assert_eq!(
        user_info,
        UserInfo {
            amount: Uint128::from(50u128),
            reward_debt: Uint128::from(0u128),
        }
    );

    env.block.height = env.block.height.add(1000);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(25u128),
        },
    )
    .unwrap();
    assert_eq!(4, res.messages.len());

    let transfer_msg = res.messages.get(2).expect("no message");
    let transfer_from_msg = res.messages.get(3).expect("no message");
    assert_eq!(
        transfer_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(100000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        },
    );
    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(25u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        },
    );
    // mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    let user_info = USER_INFO
        .load(
            deps.as_ref().storage,
            (
                &Addr::unchecked("lp-token000"),
                &Addr::unchecked("user0000"),
            ),
        )
        .unwrap();
    assert_eq!(
        user_info,
        UserInfo {
            amount: Uint128::from(25u128),
            reward_debt: Uint128::from(50000u128),
        }
    );
    env.block.height = env.block.height.add(1000);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(25u128),
        },
    )
    .unwrap();
    assert_eq!(4, res.messages.len());
    let transfer_msg = res.messages.get(2).expect("no message");
    let transfer_from_msg = res.messages.get(3).expect("no message");

    assert_eq!(
        transfer_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(10000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }
    );
    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(25u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }
    );

    // mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    let wres = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract,
            amount: Uint128::from(25u128),
        },
    );
    match wres {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::BalanceTooSmall { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn execute_emergency_withdraw() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);

    let user = Addr::unchecked("user0000");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    let token_amount = Uint128::from(10u128);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        token_amount,
        env.block.height,
        env.block.height.add(1000),
    );

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 10,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    info.sender = user.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(1122u128),
        },
    )
    .unwrap();
    assert_eq!(1, res.messages.len());
    execute_messages_token_contract(&mut deps, res);

    env.block.height = env.block.height.add(1000);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::EmergencyWithdraw {
            token: lp_token_contract.clone(),
        },
    )
    .unwrap();
    assert_eq!(1, res.messages.len());
    let transfer_from_msg = res.messages.get(0).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(1122u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }
    );
    execute_messages_token_contract(&mut deps, res);

    let user_info = USER_INFO.load(
        deps.as_ref().storage,
        (&lp_token_contract.clone(), &user.clone()),
    );
    match user_info {
        Ok(_) => panic!("Must return error"),
        Err(StdError::NotFound { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

//should give out token only after farming time
#[test]
fn give_token_after_farming_time() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let owner = Addr::unchecked("addr0000");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    deps.querier.set_balance(
        owner.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );

    // 100 per block farming rate starting at block 100 with bonus until block 1000
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(100u128),
        env.block.height.add(100),
        env.block.height.add(1000),
    );

    let _ = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 100,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(100u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // Block 90
    env.block.height = env.block.height.add(90);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Block 95
    env.block.height = env.block.height.add(5);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Block 99
    env.block.height = env.block.height.add(4);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    //Block 100
    env.block.height = env.block.height.add(1);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    //Block 101
    env.block.height = env.block.height.add(1);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 3);
    let transfer_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_msg,
        &SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: MOCK_CONTRACT_ADDR.to_string(),
                    amount: Uint128::from(1000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        },
    );
    execute_messages_token_contract(&mut deps, res);
    // Block 105
    env.block.height = env.block.height.add(4);

    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::zero(),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: dev.to_string(),
                        amount: Uint128::from(400u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: MOCK_CONTRACT_ADDR.to_string(),
                        amount: Uint128::from(4000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: owner.to_string(),
                        amount: Uint128::from(4000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            }
        ]
    );
    execute_messages_token_contract(&mut deps, res);
    // astra token balanceOf contract address equal 5000
    let bal = get_token_balance(deps.as_ref(), owner.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::from(5000u128));
    // astra token balanceOf dev address equal 500
    let bal = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::from(500u128))
    // astra token totalSupply equal 5500
}

// should not distribute tokens if no one deposit
#[test]
fn not_distribute_tokens() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);

    let user = Addr::unchecked("user0000");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    deps.querier.set_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    // 100 per block farming rate starting at block 200 with bonus until block 1000
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(100u128),
        env.block.height.add(200),
        env.block.height.add(1000),
    );

    let _ = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 100,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();
    // Block 199
    env.block.height = env.block.height.add(199);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::MassUpdatePools {},
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Block 204
    env.block.height = env.block.height.add(5);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::MassUpdatePools {},
    )
    .unwrap();
    assert_eq!(res.messages.len(), 0);
    // Block 210
    info.sender = user.clone();
    env.block.height = env.block.height.add(6);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                    amount: Uint128::from(10u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }]
    );
    execute_messages_token_contract(&mut deps, res);
    // astra token totalSupply equal 0
    // astra token balanceOf contract address equal 0
    let bal = get_token_balance(deps.as_ref(), user.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::zero());
    // astra token balanceOf dev address equal 0
    let bal = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::zero());
    // lp token balanceOf equal -10
    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(990u128));

    // Block 220
    env.block.height = env.block.height.add(10);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: dev.to_string(),
                        amount: Uint128::from(1000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                        amount: Uint128::from(10000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: info.sender.to_string(),
                        amount: Uint128::from(10000u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: lp_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: info.sender.to_string(),
                        amount: Uint128::from(10u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            }
        ]
    );
    // mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    // astra token totalSupply equal 11000
    // astra token balanceOf user address equal 10000
    let balance = get_token_balance(deps.as_ref(), user.clone(), astro_token_contract.clone());
    assert_eq!(balance, Uint128::from(10000u128));
    // astra token balanceOf dev address equal 1000
    let balance = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(balance, Uint128::from(1000u128));
    // lp token balanceOf equal +10
    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(1000u128));
}

// should distribute tokens properly for each staker
#[test]
fn distribute_tokens() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);

    let user_one = Addr::unchecked("user0000");
    let user_two = Addr::unchecked("user0001");
    let user_three = Addr::unchecked("user0002");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    deps.querier.set_balance(
        user_one.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    deps.querier.set_balance(
        user_two.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    deps.querier.set_balance(
        user_three.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    // 100 per block farming rate starting at block 300 with bonus until block 1000
    env.block.height = 0;
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(100u128),
        env.block.height.add(300),
        env.block.height.add(1000),
    );
    // Add first LP to the pool with allocation 1
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 100,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: true,
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_one deposits 10 LPs at block 310
    // Block +310
    info.sender = user_one.clone();
    env.block.height = env.block.height.add(310);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_two deposits 20 LPs at block 314
    // Bloc +314
    info.sender = user_two.clone();
    env.block.height = env.block.height.add(4);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(20u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_three deposits 30 LPs at block 318
    // Block +318
    info.sender = user_three.clone();
    env.block.height = env.block.height.add(4);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(30u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_one deposits 10 more LPs at block 320. At this point:
    // User_one should have: 4*1000 + 4*1/3*1000 + 2*1/6*1000 = 5666
    // Gauge contract  should have the remaining: 10000 - 5666 = 4334
    // Block +320
    info.sender = user_one.clone();
    env.block.height = env.block.height.add(2);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);
    // expect token totalSupply equal 11000
    // expect token balanceOf User_one address equal 5666
    let bal = get_token_balance(
        deps.as_ref(),
        user_one.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(5666u128));
    // expect token balanceOf User_two address equal 0
    let bal = get_token_balance(
        deps.as_ref(),
        user_two.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::zero());
    // expect token balanceOf User_three address equal 0
    let bal = get_token_balance(
        deps.as_ref(),
        user_three.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::zero());
    // expect token balanceOf chef address equal 4334
    let bal = get_token_balance(
        deps.as_ref(),
        Addr::unchecked(MOCK_CONTRACT_ADDR),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(4334u128));
    // expect token balanceOf dev address equal 1000
    let bal = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::from(1000u128));

    // User_two withdraws 5 LPs at block 330. At this point:
    // User_two should have: 4*2/3*1000 + 2*2/6*1000 + 10*2/7*1000 = 6190
    // Block 330
    info.sender = user_two.clone();
    env.block.height = env.block.height.add(10);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(5u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);
    // expect token totalSuppl equal 22000
    // expect token balanceOf User_one address equal 5666
    let bal = get_token_balance(
        deps.as_ref(),
        user_one.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(5666u128));
    // expect token balanceOf User_two address equal 6190
    let bal = get_token_balance(
        deps.as_ref(),
        user_two.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(6190u128));
    // expect token balanceOf User_three address equal 0
    let bal = get_token_balance(
        deps.as_ref(),
        user_three.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::zero());
    // expect token balanceOf contract address equal 8144
    let bal = get_token_balance(
        deps.as_ref(),
        Addr::unchecked(MOCK_CONTRACT_ADDR),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(8144u128));
    // expect token balanceOf dev address equal 2000
    let bal = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::from(2000u128));

    // User_one withdraws 20 LPs at block 340.
    // User_two withdraws 15 LPs at block 350.
    // User_three withdraws 30 LPs at block 360.
    // Block +340
    info.sender = user_one.clone();
    env.block.height = env.block.height.add(10);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(20u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // Block +350
    info.sender = user_two.clone();
    env.block.height = env.block.height.add(10);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(15u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // Block +360
    info.sender = user_three.clone();
    env.block.height = env.block.height.add(10);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Withdraw {
            token: lp_token_contract.clone(),
            amount: Uint128::from(30u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // expect token totalSupply equal 55000
    // expect token balanceOf dev address equal 5000
    let bal = get_token_balance(deps.as_ref(), dev.clone(), astro_token_contract.clone());
    assert_eq!(bal, Uint128::from(5000u128));

    // User_one should have: 5666 + 10*2/7*1000 + 10*2/6.5*1000 = 11600
    // expect token balanceOf User_one address equal 11600
    let bal = get_token_balance(
        deps.as_ref(),
        user_one.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(11600u128));

    // User_two should have: 6190 + 10*1.5/6.5 * 1000 + 10*1.5/4.5*1000 = 11831
    // expect token balanceOf User_two address equal 11831
    let bal = get_token_balance(
        deps.as_ref(),
        user_two.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(11831u128));

    // User_three should have: 2*3/6*1000 + 10*3/7*1000 + 10*3/6.5*1000 + 10*3/4.5*1000 + 10*1000 = 26568
    // expect token balanceOf User_three address equal 26568
    let bal = get_token_balance(
        deps.as_ref(),
        user_three.clone(),
        astro_token_contract.clone(),
    );
    assert_eq!(bal, Uint128::from(26568u128));

    // All of them should have 1000 LPs back.
    // expect lp token balanceOf User_one address equal 1000
    let bal = get_token_balance(deps.as_ref(), user_one.clone(), lp_token_contract.clone());
    assert_eq!(bal, Uint128::from(1000u128));
    // expect lp token balanceOf User_two address equal 1000
    let bal = get_token_balance(deps.as_ref(), user_two.clone(), lp_token_contract.clone());
    assert_eq!(bal, Uint128::from(1000u128));
    // expect lp token balanceOf User_three address equal 1000
    let bal = get_token_balance(deps.as_ref(), user_three.clone(), lp_token_contract.clone());
    assert_eq!(bal, Uint128::from(1000u128));
}

// should give proper tokens allocation to each pool
#[test]
fn tokens_allocation_each_pool() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = Addr::unchecked("addr0000");
    let user_one = Addr::unchecked("user0000");
    let user_two = Addr::unchecked("user0001");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract_one = Addr::unchecked("lp-token000");
    let lp_token_contract_two = Addr::unchecked("lp-token001");

    deps.querier.set_balance(
        user_one.clone(),
        lp_token_contract_one.clone(),
        Uint128::from(1000u128),
    );
    deps.querier.set_balance(
        user_two.clone(),
        lp_token_contract_two.clone(),
        Uint128::from(1000u128),
    );
    // 100 per block farming rate starting at block 400 with bonus until block 1000
    env.block.height = 0;
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(100u128),
        env.block.height.add(400),
        env.block.height.add(1000),
    );
    // Add first LP to the pool with allocation 1
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 10,
            token: lp_token_contract_one.clone(),
            reward_proxy: None,
            with_update: true,
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_one deposits 10 LPs at block 410
    // Block +410
    info.sender = user_one.clone();
    env.block.height = env.block.height.add(410);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract_one.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // Add LP2 to the pool with allocation 2 at block 420
    // Block +420
    env.block.height = env.block.height.add(10);
    info.sender = owner.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 20,
            token: lp_token_contract_two.clone(),
            reward_proxy: None,
            with_update: true,
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // User_one should have 10*1000 pending reward
    info.sender = user_one.clone();
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract_one.clone(),
            user: user_one.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::from(10000u128));

    // User_two deposits 10 LP2s at block 425
    // Block +425
    env.block.height = env.block.height.add(5);
    info.sender = user_two.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract_two.clone(),
            amount: Uint128::from(5u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    //User_one should have 10000 + 5*1/3*1000 = 11666 pending reward
    info.sender = user_one.clone();
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract_one.clone(),
            user: user_one.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::from(11666u128));

    // Block +430
    env.block.height = env.block.height.add(5);
    // At block 430. User_two should get 5*2/3*1000 = 3333. User_one should get ~1666 more.
    // expect pending token one user one address equal 13333
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract_one.clone(),
            user: user_one.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::from(13333u128));
    // expect pending token two user two address equal 3333
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract_two.clone(),
            user: user_two.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::from(3333u128));
}

//should stop giving bonus tokens after the bonus period ends
#[test]
fn stop_giving_bonus_tokens() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);

    let user = Addr::unchecked("user0000");
    let dev = Addr::unchecked("dev0000");
    let mut env = mock_env();
    let astro_token_contract = Addr::unchecked("astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    deps.querier.set_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    // 100 per block farming rate starting at block 500 with bonus until block 600
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        dev.clone(),
        astro_token_contract.clone(),
        Uint128::from(100u128),
        env.block.height.add(500),
        env.block.height.add(600),
    );

    let _ = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Add {
            alloc_point: 1,
            token: lp_token_contract.clone(),
            reward_proxy: None,
            with_update: false,
        },
    )
    .unwrap();

    // User deposits 10 LPs at block +590
    // Block +590
    env.block.height = env.block.height.add(590);
    info.sender = user.clone();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(10u128),
        },
    )
    .unwrap();
    execute_messages_token_contract(&mut deps, res);

    // At block 605, he should have 1000*10 + 100*5 = 10500 pending.
    // Block +605
    env.block.height = env.block.height.add(15);
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract.clone(),
            user: user.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::from(10500u128));
    // At block 606, user withdraws all pending rewards and should get 10600.
    env.block.height = env.block.height.add(1);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Deposit {
            token: lp_token_contract.clone(),
            amount: Uint128::from(0u128),
        },
    )
    .unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: dev.to_string(),
                        amount: Uint128::from(1060u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                        amount: Uint128::from(10600u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: astro_token_contract.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: info.sender.to_string(),
                        amount: Uint128::from(10600u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
        ]
    );
    // mock execute messages cw20 token contract
    execute_messages_token_contract(&mut deps, res);

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::PendingToken {
            token: lp_token_contract.clone(),
            user: user.clone(),
        },
    )
    .unwrap();
    let rewards: PendingTokenResponse = from_binary(&res).unwrap();
    assert_eq!(rewards.pending, Uint128::zero());
}

#[test]
fn check_mock_query_balances() {
    let mut deps = mock_dependencies(&[]);
    let user = Addr::unchecked("user0000");
    let user_other = Addr::unchecked("user0001");
    let lp_token_contract = Addr::unchecked("lp-token000");
    let lp_token_contract_other = Addr::unchecked("lp-token001");

    deps.querier.set_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );

    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(1000u128));
    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract_other.clone());
    assert_eq!(balance, Uint128::zero());
    let balance = get_token_balance(deps.as_ref(), user_other.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::zero());
    let balance = get_token_balance(
        deps.as_ref(),
        user_other.clone(),
        lp_token_contract_other.clone(),
    );
    assert_eq!(balance, Uint128::zero());

    deps.querier.add_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(2000u128));

    deps.querier.add_balance(
        user_other.clone(),
        lp_token_contract.clone(),
        Uint128::from(1000u128),
    );
    let balance = get_token_balance(deps.as_ref(), user_other.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(1000u128));

    deps.querier.sub_balance(
        user.clone(),
        lp_token_contract.clone(),
        Uint128::from(1500u128),
    );
    let balance = get_token_balance(deps.as_ref(), user.clone(), lp_token_contract.clone());
    assert_eq!(balance, Uint128::from(500u128));

    deps.querier.sub_balance(
        user_other.clone(),
        lp_token_contract_other.clone(),
        Uint128::from(1000u128),
    );
    let balance = get_token_balance(
        deps.as_ref(),
        user_other.clone(),
        lp_token_contract_other.clone(),
    );
    assert_eq!(balance, Uint128::zero())
}
