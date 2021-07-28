use std::ops::Add;

use cosmwasm_std::{Addr, Api, attr, CosmosMsg, Deps, DepsMut, Env, from_binary, MessageInfo, to_binary, Uint128, WasmMsg};
use cosmwasm_std::testing::{MOCK_CONTRACT_ADDR, mock_env, mock_info};
use cw20::Cw20ExecuteMsg;

use crate::contract::{add, deposit, emergency_withdraw, execute, instantiate, pool_length, query, withdraw};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::msg::{ExecuteMsg, InstantiateMsg, PoolLengthResponse, QueryMsg};
use crate::state::{CONFIG, Config, POOL_INFO, PoolInfo, USER_INFO, UserInfo};

fn get_length(deps: Deps) -> usize {
    pool_length(deps).unwrap().length
}

fn get_addr(api: &dyn Api, s: &str) -> Addr {
    let owner_raw = api.addr_canonicalize(s).unwrap();
    api.addr_humanize(&owner_raw).unwrap()
}

fn _do_instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Addr,
    x_astro_toke: Addr,
    _tokens_per_block: Uint128,
    _start_block: u64,
    _bonus_end_block: u64,
) {
    let instantiate_msg = InstantiateMsg {
        token: x_astro_toke,
        dev_addr: owner,
        tokens_per_block: _tokens_per_block,
        start_block: _start_block,
        bonus_end_block: _bonus_end_block,
    };
    let res = instantiate(deps, _env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let env = mock_env();
    let x_astro_token_contract = Addr::unchecked("x_astro-token");
    let token_amount = Uint128(10);

    let instantiate_msg = InstantiateMsg {
        token: x_astro_token_contract,
        dev_addr: owner,
        tokens_per_block: token_amount,
        start_block: 2,
        bonus_end_block: 10,
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        config,
        Config {
            owner: Addr::unchecked("addr0000"),
            x_astro_token: Addr::unchecked("x_astro-token"),
            dev_addr: Addr::unchecked("addr0000"),
            bonus_end_block: 10,
            tokens_per_block: token_amount,
            total_alloc_point: 0,
            start_block: 2,
        }
    )
}

#[test]
fn execute_add() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = Addr::unchecked("addr0000");
    let user = Addr::unchecked("addr0001");
    let env = mock_env();
    let x_astro_token_contract = Addr::unchecked("x_astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");
    let lp_token_contract1 = Addr::unchecked("lp-token001");
    let token_amount = Uint128(10);
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        x_astro_token_contract.clone(),
        token_amount,
        env.block.height.add(10),
        env.block.height.add(110),
    );

    let res = query(deps.as_ref(), env.clone(), QueryMsg::PoolLength {}).unwrap();
    let pool_length: PoolLengthResponse = from_binary(&res).unwrap();
    assert_eq!(pool_length.length, 0);

    let msg = ExecuteMsg::Add { alloc_point: 10, token: lp_token_contract.clone() };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract.clone()).unwrap();

    assert_eq!(
        config,
        Config {
            owner: owner.clone(),
            x_astro_token: Addr::unchecked("x_astro-token"),
            dev_addr: owner.clone(),
            bonus_end_block: env.block.height.add(110),
            tokens_per_block: Uint128(10),
            total_alloc_point: 10,
            start_block: env.block.height.add(10),
        }
    );
    assert_eq!(
        pool_info,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
        }
    );
    let res = query(deps.as_ref(), env.clone(), QueryMsg::PoolLength {}).unwrap();
    let pool_length: PoolLengthResponse = from_binary(&res).unwrap();
    assert_eq!(pool_length.length, 1);

    let msg = ExecuteMsg::Add { alloc_point: 20, token: lp_token_contract.clone() };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(ContractError::TokenPoolAlreadyExists { .. }) => {}
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    info.sender = user;
    let msg = ExecuteMsg::Add { alloc_point: 20, token: lp_token_contract1.clone() };
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
    let pool_info1 = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract.clone()).unwrap();
    let pool_info2 = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract1.clone()).unwrap();

    assert_eq!(
        config,
        Config {
            owner: owner.clone(),
            x_astro_token: Addr::unchecked("x_astro-token"),
            dev_addr: owner.clone(),
            bonus_end_block: env.block.height.add(110),
            tokens_per_block: Uint128(10),
            total_alloc_point: 10 + 20,
            start_block: env.block.height.add(10),
        }
    );
    assert_eq!(
        pool_info1,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
        }
    );
    assert_eq!(
        pool_info2,
        PoolInfo {
            alloc_point: 20,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
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
    let env = mock_env();
    let x_astro_token_contract = Addr::unchecked("x_astro-token");
    let lp_token_contract = Addr::unchecked("lp-token000");

    let token_amount = Uint128(10);
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        x_astro_token_contract.clone(),
        token_amount,
        env.block.height.add(10),
        env.block.height.add(110),
    );

    let msg = ExecuteMsg::Add { alloc_point: 10, token: lp_token_contract.clone() };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract.clone()).unwrap();
    assert_eq!(
        config.total_alloc_point,
        10
    );
    assert_eq!(
        pool_info,
        PoolInfo {
            alloc_point: 10,
            last_reward_block: env.block.height.add(10),
            acc_per_share: Default::default(),
        }
    );

    info.sender = get_addr(deps.as_mut().api, "addr0001");
    let wr = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Set {
            token: lp_token_contract.clone(),
            alloc_point: 20,
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
        },
    ).unwrap();
    assert_eq!(res.messages.len(), 0);
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract.clone()).unwrap();
    assert_eq!(config.total_alloc_point, 20);
    assert_eq!(pool_info.alloc_point, 20);

    let msg = ExecuteMsg::Add { alloc_point: 100, token: Addr::unchecked("come_token") };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Set {
            token: lp_token_contract.clone(),
            alloc_point: 50,
        },
    ).unwrap();
    assert_eq!(res.messages.len(), 0);
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    let pool_info = POOL_INFO.load(deps.as_ref().storage, &lp_token_contract.clone()).unwrap();
    assert_eq!(config.total_alloc_point, 150); //old: token120+token2 100; new: total 120 -20+50 token1
    assert_eq!(pool_info.alloc_point, 50);
}

#[test]
fn execute_deposit() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let mut env = mock_env();
    let x_astro_token_contract = get_addr(deps.as_mut().api, "x_astro-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(100);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        x_astro_token_contract.clone(),
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
        },
    ).unwrap();

    let msg = ExecuteMsg::Deposit {
        token: lp_token_contract.clone(),
        amount: Uint128(1000),
    };
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        msg,
    ).unwrap();
    let transfer_from_msg = res.messages.get(0).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(1000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        res.attributes,
        vec![
            attr("Action", "Deposit"),
        ],
    );
    let user_info = USER_INFO.load(
        deps.as_ref().storage,
        (&Addr::unchecked("lp-token000"), &Addr::unchecked("addr0000"))
    )
        .unwrap();

    assert_eq!(
        user_info,
        UserInfo{
            amount: Uint128(1000),
            reward_debt: Uint128::zero(),
        }
    );


    let msg = ExecuteMsg::Deposit {
        token: lp_token_contract.clone(),
        amount: Uint128(2000),
    };
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        msg,
    ).unwrap();
    let mut transfer_msg = res.messages.get(0).expect("no message");
    let transfer_from_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: x_astro_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128::zero(),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(2000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    env.block.height = env.block.height.add(50);

    let msg = ExecuteMsg::Deposit {
        token: lp_token_contract.clone(),
        amount: Uint128(2000),
    };
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        msg,
    ).unwrap();
    assert_eq!(4, res.messages.len());
    assert_eq!(
         res.messages,
         vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: x_astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: owner.to_string(),
                    amount: Uint128(5000),
                })
                    .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: x_astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                    amount: Uint128(50000),
                })
                    .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: x_astro_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: Uint128(49998),
                })
                    .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                    amount: Uint128(3000),
                })
                    .unwrap(),
                send: vec![],
            })
        ]
    );
}

#[test]
fn execute_withdraw() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let mut env = mock_env();
    let x_astro_token_contract = get_addr(deps.as_mut().api, "x_astro-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(10u128);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        x_astro_token_contract.clone(),
        token_amount,
        env.block.height,
        env.block.height.add(1000),
    );

    let mut res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract.clone(),
        //false,
    )
        .unwrap();
    assert_eq!(0, res.messages.len());
    res = deposit(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
        Uint128(100u128),
    )
        .unwrap();
    assert_eq!(1, res.messages.len());
    assert_eq!(attr("Action", "Deposit"), res.attributes[0]);

    res = withdraw(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
        Uint128(50u128),
    )
        .unwrap();
    assert_eq!(2, res.messages.len());
    let mut transfer_msg = res.messages.get(0).expect("no message");
    let mut transfer_from_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: x_astro_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128::zero(),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: MOCK_CONTRACT_ADDR.parse().unwrap(),
                recipient: info.sender.to_string(),
                amount: Uint128(50u128),
            })
                .unwrap(),
            send: vec![],
        })
    );
    env.block.height = env.block.height.add(1000);
    res = withdraw(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
        Uint128(25u128),
    )
        .unwrap();
    assert_eq!(4, res.messages.len());

    transfer_msg = res.messages.get(2).expect("no message");
    transfer_from_msg = res.messages.get(3).expect("no message");
    for i in res.attributes {
        println!("{} {}", i.key, i.value);
    }
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: x_astro_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128(100000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: MOCK_CONTRACT_ADDR.parse().unwrap(),
                recipient: info.sender.to_string(),
                amount: Uint128(25u128),
            })
                .unwrap(),
            send: vec![],
        })
    );
    env.block.height = env.block.height.add(1000);
    res = withdraw(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
        Uint128(25u128),
    )
        .unwrap();
    assert_eq!(4, res.messages.len());
    transfer_msg = res.messages.get(2).expect("no message");
    transfer_from_msg = res.messages.get(3).expect("no message");

    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: x_astro_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: Uint128(60000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: MOCK_CONTRACT_ADDR.parse().unwrap(),
                recipient: info.sender.to_string(),
                amount: Uint128(25u128),
            })
                .unwrap(),
            send: vec![],
        })
    );
    let wres = withdraw(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract,
        Uint128(25u128),
    );
    match wres.unwrap_err() {
        ContractError::BalanceTooSmall {} => {}
        e => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn execute_emergency_withdraw() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let owner = get_addr(deps.as_mut().api, "addr0000");
    let mut env = mock_env();
    let x_astro_token_contract = get_addr(deps.as_mut().api, "x_astro-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(10);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        x_astro_token_contract.clone(),
        token_amount,
        env.block.height,
        env.block.height.add(1000),
    );

    let mut res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract.clone(),
        //false,
    )
        .unwrap();
    assert_eq!(0, res.messages.len());
    res = deposit(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
        Uint128(1122),
    )
        .unwrap();
    assert_eq!(1, res.messages.len());
    env.block.height = env.block.height.add(1000);
    res = emergency_withdraw(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        lp_token_contract.clone(),
    )
        .unwrap();
    assert_eq!(1, res.messages.len());
    // for i in res.attributes{
    //     println!("{} {}", i.key, i.value);
    // }
    let transfer_from_msg = res.messages.get(0).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: MOCK_CONTRACT_ADDR.parse().unwrap(),
                recipient: info.sender.to_string(),
                amount: Uint128(1122),
            })
                .unwrap(),
            send: vec![],
        })
    );
}
