// SPDX-License-Identifier: GPL-3.0-only
// Copyright Astroport
// Copyright Lido

use super::mock_querier::mock_dependencies as dependencies;
use crate::contract::{
    accumulate_prices, execute, instantiate, query_reverse_simulation, query_simulation,
};
use crate::msgs::InstantiateMsg;
use crate::state::Config;
use crate::testing::mock_querier::{
    MOCK_BLUNA_TOKEN_CONTRACT_ADDR, MOCK_HUB_CONTRACT_ADDR, MOCK_STLUNA_TOKEN_CONTRACT_ADDR,
};
use astroport::asset::{Asset, AssetInfo};
use astroport::pair::ExecuteMsg::Receive;
use astroport::pair::TWAP_PRECISION;
use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{
    to_binary, Addr, Api, BlockInfo, CosmosMsg, Env, OwnedDeps, Querier, Storage, Timestamp,
    Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use std::borrow::BorrowMut;

pub fn initialize<S: Storage, A: Api, Q: Querier>(deps: &mut OwnedDeps<S, A, Q>) {
    let msg = InstantiateMsg {
        stluna_address: MOCK_STLUNA_TOKEN_CONTRACT_ADDR.to_string(),
        bluna_address: MOCK_BLUNA_TOKEN_CONTRACT_ADDR.to_string(),
        hub_address: MOCK_HUB_CONTRACT_ADDR.to_string(),
    };

    let owner_info = mock_info("owner", &[]);
    instantiate(deps.as_mut(), mock_env(), owner_info, msg).unwrap();
}

#[test]
fn proper_swap_stluna_bluna() {
    let mut deps = dependencies(&[]);

    initialize(deps.borrow_mut());

    let sender = "addr";
    let amount = Uint128::from(100u128);

    let stluna_info = mock_info(MOCK_STLUNA_TOKEN_CONTRACT_ADDR, &[]);
    let swap = astroport::pair::Cw20HookMsg::Swap {
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let receive = Receive(Cw20ReceiveMsg {
        sender: sender.to_string(),
        amount,
        msg: to_binary(&swap).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), stluna_info, receive).unwrap();
    assert_eq!(1, res.messages.len());

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, MOCK_STLUNA_TOKEN_CONTRACT_ADDR);
            assert_eq!(
                msg,
                to_binary(&Cw20ExecuteMsg::Send {
                    contract: MOCK_HUB_CONTRACT_ADDR.to_string(),
                    amount,
                    msg: to_binary(&basset::hub::Cw20HookMsg::Convert {}).unwrap()
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }
}

#[test]
fn proper_swap_bluna_stluna() {
    let mut deps = dependencies(&[]);

    initialize(deps.borrow_mut());

    let sender = "addr";
    let amount = Uint128::from(100u128);

    let stluna_info = mock_info(MOCK_BLUNA_TOKEN_CONTRACT_ADDR, &[]);
    let swap = astroport::pair::Cw20HookMsg::Swap {
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let receive = Receive(Cw20ReceiveMsg {
        sender: sender.to_string(),
        amount,
        msg: to_binary(&swap).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), stluna_info, receive).unwrap();
    assert_eq!(1, res.messages.len());

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, MOCK_BLUNA_TOKEN_CONTRACT_ADDR);
            assert_eq!(
                msg,
                to_binary(&Cw20ExecuteMsg::Send {
                    contract: MOCK_HUB_CONTRACT_ADDR.to_string(),
                    amount,
                    msg: to_binary(&basset::hub::Cw20HookMsg::Convert {}).unwrap()
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }
}

#[test]
fn proper_simulation_query() {
    let mut deps = dependencies(&[]);

    initialize(deps.borrow_mut());

    let bluna_amount = Uint128::from(150u128);
    let expected_return_stluna_amount = Uint128::from(90u128);
    let simulation_response = query_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_BLUNA_TOKEN_CONTRACT_ADDR),
            },
            amount: bluna_amount,
        },
    )
    .unwrap();
    assert_eq!(
        expected_return_stluna_amount,
        simulation_response.return_amount
    );

    let stluna_amount = Uint128::from(100u128);
    let expected_return_bluna_amount = Uint128::from(150u128);
    let simulation_response = query_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_STLUNA_TOKEN_CONTRACT_ADDR),
            },
            amount: stluna_amount,
        },
    )
    .unwrap();
    assert_eq!(
        expected_return_bluna_amount,
        simulation_response.return_amount
    )
}

#[test]
fn proper_reverse_simulation_query() {
    let mut deps = dependencies(&[]);

    initialize(deps.borrow_mut());

    let bluna_amount = Uint128::from(150u128);
    let expected_offer_stluna_amount = Uint128::from(100u128);
    let simulation_response = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_BLUNA_TOKEN_CONTRACT_ADDR),
            },
            amount: bluna_amount,
        },
    )
    .unwrap();
    assert_eq!(
        expected_offer_stluna_amount,
        simulation_response.offer_amount
    );

    let stluna_amount = Uint128::from(90u128);
    let expected_offer_bluna_amount = Uint128::from(149u128); // ~150
    let simulation_response = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_STLUNA_TOKEN_CONTRACT_ADDR),
            },
            amount: stluna_amount,
        },
    )
    .unwrap();
    assert_eq!(
        expected_offer_bluna_amount,
        simulation_response.offer_amount
    )
}

#[test]
fn test_accumulate_prices() {
    struct Case {
        block_time: u64,
        block_time_last: u64,
        last0: u128,
        last1: u128,
    }

    struct Result {
        block_time_last: u64,
        price_x: u128,
        price_y: u128,
        is_some: bool,
    }

    let price_precision = 10u128.pow(TWAP_PRECISION.into());

    let test_cases: Vec<(Case, Result)> = vec![
        (
            Case {
                block_time: 1000,
                block_time_last: 0,
                last0: 0,
                last1: 0,
            },
            Result {
                block_time_last: 1000,
                price_x: 1500,
                price_y: 633,
                is_some: true,
            },
        ),
        // Same block height, no changes
        (
            Case {
                block_time: 1000,
                block_time_last: 1000,
                last0: price_precision,
                last1: 2 * price_precision,
            },
            Result {
                block_time_last: 1000,
                price_x: 1,
                price_y: 2,
                is_some: false,
            },
        ),
        (
            Case {
                block_time: 1500,
                block_time_last: 1000,
                last0: 1500 * price_precision,
                last1: 633 * price_precision,
            },
            Result {
                block_time_last: 1500,
                price_x: 2250,
                price_y: 949,
                is_some: true,
            },
        ),
    ];

    let deps = dependencies(&[]);

    for test_case in test_cases {
        let (case, result) = test_case;

        let env = mock_env_with_block_time(case.block_time);
        let config = accumulate_prices(
            deps.as_ref(),
            env,
            &Config {
                block_time_last: case.block_time_last,
                price0_cumulative_last: Uint128::new(case.last0),
                price1_cumulative_last: Uint128::new(case.last1),
                hub_addr: Addr::unchecked(MOCK_HUB_CONTRACT_ADDR),
                stluna_addr: Addr::unchecked(MOCK_STLUNA_TOKEN_CONTRACT_ADDR),
                bluna_addr: Addr::unchecked(MOCK_BLUNA_TOKEN_CONTRACT_ADDR),
                owner: Addr::unchecked("owner"),
            },
        )
        .unwrap();

        assert_eq!(result.is_some, config.is_some());

        if let Some(config) = config {
            assert_eq!(config.2, result.block_time_last);
            assert_eq!(
                config.0 / Uint128::from(price_precision),
                Uint128::new(result.price_x)
            );
            assert_eq!(
                config.1 / Uint128::from(price_precision),
                Uint128::new(result.price_y)
            );
        }
    }
}

fn mock_env_with_block_time(time: u64) -> Env {
    let mut env = mock_env();
    env.block = BlockInfo {
        height: 1,
        time: Timestamp::from_seconds(time),
        chain_id: "columbus".to_string(),
    };
    env
}
