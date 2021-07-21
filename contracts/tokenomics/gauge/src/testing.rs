use crate::contract::{instantiate, pool_length, add, set, deposit, withdraw, emergency_withdraw};
use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{attr, Addr, Api, Deps, DepsMut, Env, MessageInfo, Uint128, CosmosMsg, WasmMsg, to_binary};
use std::ops::Add;
use cw20::Cw20ExecuteMsg;

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
    xrts_toke: Addr,
    _tokens_per_block: Uint128,
    _start_block: u64,
    _bonus_end_block: u64,
) {
    let instantiate_msg = InstantiateMsg {
        token: xrts_toke,
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
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let token_amount = Uint128(10);

    let instantiate_msg = InstantiateMsg {
        token: xrts_token_contract,
        dev_addr: owner,
        tokens_per_block: token_amount,
        start_block: 2,
        bonus_end_block: 10,
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
fn execute_add() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let user = get_addr(deps.as_mut().api, "addr0001");
    let env = mock_env();
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");
    let lp_token_contract1 = get_addr(deps.as_mut().api, "lp-token001");
    let token_amount = Uint128(10);
    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        xrts_token_contract.clone(),
        token_amount,
        env.block.height.add(10),
        env.block.height.add(110),
    );
    let mut pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 0);
    let mut res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract,
        false,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 1);

    info.sender = user;
    let wr = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract1.clone(),
        false,
    );
    match wr.unwrap_err() {
        ContractError::Unauthorized {} => {}
        e => panic!("Unexpected error: {:?}", e),
    }
    info.sender = owner;
    res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract1,
        true,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 2)
}

#[test]
fn execute_set() {
    let mut deps = mock_dependencies(&[]);
    let mut info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let env = mock_env();
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(10);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        xrts_token_contract.clone(),
        token_amount,
        2,
        10,
    );
    let mut pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 0);
    let mut res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract,
        false,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 1);

    info.sender = get_addr(deps.as_mut().api, "addr0001");
    let wr = set(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        0,
        20,
        false,
    );
    match wr.unwrap_err() {
        ContractError::Unauthorized {} => {}
        e => panic!("Unexpected error: {:?}", e),
    }
    info.sender = owner;
    res = set(deps.as_mut(), env.clone(), info.clone(), 0, 20, true).unwrap();
    assert_eq!(0, res.messages.len())
}

#[test]
fn execute_deposit() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let mut env = mock_env();
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(100);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        xrts_token_contract.clone(),
        token_amount,
        env.block.height,
        env.block.height.add(100),
    );
    let mut pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 0);
    let mut res = add(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        10,
        lp_token_contract.clone(),
        false,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    pool_length = get_length(deps.as_ref());
    assert_eq!(pool_length, 1);

    res = deposit(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(1000)).unwrap();
    assert_eq!(1, res.messages.len());
    let mut transfer_from_msg = res.messages.get(0).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner:info.sender.to_string(),
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(1000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    res = deposit(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(2000)).unwrap();
    assert_eq!(2, res.messages.len());

    let mut transfer_msg = res.messages.get(0).expect("no message");
    transfer_from_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient:info.sender.to_string(),
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
                owner:info.sender.to_string(),
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(2000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    env.block.height = env.block.height.add(50);
    res = deposit(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(3000)).unwrap();
    assert_eq!(4, res.messages.len());
    let mint_dev_msg = res.messages.get(0).expect("no message");
    let mint_msg = res.messages.get(1).expect("no message");
    transfer_msg = res.messages.get(2).expect("no message");
    transfer_from_msg = res.messages.get(3).expect("no message");
    // for i in res.attributes{
    //     println!("{} {}", i.key, i.value);
    // }
    assert_eq!(
        mint_dev_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: owner.to_string(),
                amount: Uint128(5000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        mint_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(50000),
            })
                .unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient:info.sender.to_string(),
                amount: Uint128(49998),
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
                owner:info.sender.to_string(),
                recipient: MOCK_CONTRACT_ADDR.parse().unwrap(),
                amount: Uint128(3000),
            })
                .unwrap(),
            send: vec![],
        })
    );
}

#[test]
fn execute_withdraw() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);
    let owner = get_addr(deps.as_mut().api, "addr0000");
    let mut env = mock_env();
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(10u128);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        xrts_token_contract.clone(),
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
        false,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    res = deposit(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(100u128)).unwrap();
    assert_eq!(1, res.messages.len());
    assert_eq!(attr("Action", "Deposit"), res.attributes[0]);

    res = withdraw(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(50u128)).unwrap();
    assert_eq!(2, res.messages.len());
    let mut transfer_msg = res.messages.get(0).expect("no message");
    let mut transfer_from_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient:info.sender.to_string(),
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
                owner:MOCK_CONTRACT_ADDR.parse().unwrap(),
                recipient: info.sender.to_string(),
                amount: Uint128(50u128),
            })
                .unwrap(),
            send: vec![],
        })
    );
    env.block.height = env.block.height.add(1000);
    res = withdraw(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(25u128)).unwrap();
    assert_eq!(4, res.messages.len());

    transfer_msg = res.messages.get(2).expect("no message");
    transfer_from_msg = res.messages.get(3).expect("no message");
    for i in res.attributes{
        println!("{} {}", i.key, i.value);
    }
    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient:info.sender.to_string(),
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
    res = withdraw(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(25u128)).unwrap();
    assert_eq!(4, res.messages.len());
    transfer_msg = res.messages.get(2).expect("no message");
    transfer_from_msg = res.messages.get(3).expect("no message");

    assert_eq!(
        transfer_msg,
        &CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xrts_token_contract.clone().to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient:info.sender.to_string(),
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
    let wres = withdraw(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(25u128));
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
    let xrts_token_contract = get_addr(deps.as_mut().api, "xrts-token");
    let lp_token_contract = get_addr(deps.as_mut().api, "lp-token000");

    let token_amount = Uint128(10);

    _do_instantiate(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        owner.clone(),
        xrts_token_contract.clone(),
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
        false,
    )
    .unwrap();
    assert_eq!(0, res.messages.len());
    res = deposit(deps.as_mut(), env.clone(), info.clone(), 0, Uint128(1122)).unwrap();
    assert_eq!(1, res.messages.len());
    env.block.height = env.block.height.add(1000);
    res = emergency_withdraw(deps.as_mut(), env.clone(), info.clone(), 0).unwrap();
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
