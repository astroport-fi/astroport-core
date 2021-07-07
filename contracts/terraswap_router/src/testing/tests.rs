use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Coin, CosmosMsg, Decimal, HumanAddr, StdError, Uint128, WasmMsg,
};

use crate::contract::{handle, init, query};
use crate::testing::mock_querier::mock_dependencies;

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use terra_cosmwasm::{create_swap_msg, create_swap_send_msg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::HandleMsg as PairHandleMsg;
use terraswap::router::{
    ConfigResponse, Cw20HookMsg, HandleMsg, InitMsg, QueryMsg, SimulateSwapOperationsResponse,
    SwapOperation,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        terraswap_factory: HumanAddr("terraswapfactory".to_string()),
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    // it worked, let's query the state
    let config: ConfigResponse = from_binary(&query(&deps, QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!("terraswapfactory", config.terraswap_factory.as_str());
}

#[test]
fn execute_swap_operations() {
    let mut deps = mock_dependencies(20, &[]);
    let msg = InitMsg {
        terraswap_factory: HumanAddr("terraswapfactory".to_string()),
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::ExecuteSwapOperations {
        operations: vec![],
        minimum_receive: None,
        to: None,
    };

    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "must provide operations"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    let msg = HandleMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::NativeSwap {
                offer_denom: "uusd".to_string(),
                ask_denom: "ukrw".to_string(),
            },
            SwapOperation::TerraSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: HumanAddr::from("asset0001"),
                },
            },
            SwapOperation::TerraSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: HumanAddr::from("asset0001"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
            SwapOperation::TerraSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: HumanAddr::from("asset0002"),
                },
            },
        ],
        minimum_receive: Some(Uint128::from(1000000u128)),
        to: None,
    };

    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::NativeSwap {
                        offer_denom: "uusd".to_string(),
                        ask_denom: "ukrw".to_string(),
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "ukrw".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                        ask_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0002"),
                        },
                    },
                    to: Some(HumanAddr::from("addr0000")),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::AssertMinimumReceive {
                    asset_info: AssetInfo::Token {
                        contract_addr: HumanAddr::from("asset0002"),
                    },
                    prev_balance: Uint128::zero(),
                    minimum_receive: Uint128::from(1000000u128),
                    receiver: HumanAddr::from("addr0000"),
                })
                .unwrap(),
            }),
        ]
    );

    let msg = HandleMsg::Receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("addr0000"),
        amount: Uint128::from(1000000u128),
        msg: Some(
            to_binary(&Cw20HookMsg::ExecuteSwapOperations {
                operations: vec![
                    SwapOperation::NativeSwap {
                        offer_denom: "uusd".to_string(),
                        ask_denom: "ukrw".to_string(),
                    },
                    SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "ukrw".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                    },
                    SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                        ask_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                    },
                    SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0002"),
                        },
                    },
                ],
                minimum_receive: None,
                to: Some(HumanAddr::from("addr0002")),
            })
            .unwrap(),
        ),
    });

    let env = mock_env("asset0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::NativeSwap {
                        offer_denom: "uusd".to_string(),
                        ask_denom: "ukrw".to_string(),
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "ukrw".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0001"),
                        },
                        ask_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                    },
                    to: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::ExecuteSwapOperation {
                    operation: SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: HumanAddr::from("asset0002"),
                        },
                    },
                    to: Some(HumanAddr::from("addr0002")),
                })
                .unwrap(),
            })
        ]
    );
}

#[test]
fn execute_swap_operation() {
    let mut deps = mock_dependencies(20, &[]);
    let msg = InitMsg {
        terraswap_factory: HumanAddr("terraswapfactory".to_string()),
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    deps.querier
        .with_terraswap_pairs(&[(&"uusdasset".to_string(), &HumanAddr::from("pair"))]);
    deps.querier.with_tax(
        Decimal::percent(5),
        &[(&"uusd".to_string(), &Uint128(1000000u128))],
    );
    deps.querier.with_balance(&[(
        &HumanAddr::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            amount: Uint128(1000000u128),
            denom: "uusd".to_string(),
        }],
    )]);

    let msg = HandleMsg::ExecuteSwapOperation {
        operation: SwapOperation::NativeSwap {
            offer_denom: "uusd".to_string(),
            ask_denom: "uluna".to_string(),
        },
        to: None,
    };
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![create_swap_msg(
            HumanAddr::from(MOCK_CONTRACT_ADDR),
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128(1000000u128),
            },
            "uluna".to_string()
        )],
    );

    // optional to address
    // swap_send
    let msg = HandleMsg::ExecuteSwapOperation {
        operation: SwapOperation::NativeSwap {
            offer_denom: "uusd".to_string(),
            ask_denom: "uluna".to_string(),
        },
        to: Some(HumanAddr::from("addr0000")),
    };
    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let res = handle(&mut deps, env, msg.clone()).unwrap();
    assert_eq!(
        res.messages,
        vec![create_swap_send_msg(
            HumanAddr::from(MOCK_CONTRACT_ADDR),
            HumanAddr::from("addr0000"),
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128(952380u128), // deduct tax
            },
            "uluna".to_string()
        )],
    );
    deps.querier
        .with_terraswap_pairs(&[(&"assetuusd".to_string(), &HumanAddr::from("pair"))]);
    deps.querier.with_token_balances(&[(
        &HumanAddr::from("asset"),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(1000000u128))],
    )]);

    let msg = HandleMsg::ExecuteSwapOperation {
        operation: SwapOperation::TerraSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: HumanAddr::from("asset"),
            },
            ask_asset_info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        to: Some(HumanAddr::from("addr0000")),
    };

    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("asset"),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: HumanAddr::from("pair"),
                amount: Uint128(1000000u128),
                msg: Some(
                    to_binary(&PairHandleMsg::Swap {
                        offer_asset: Asset {
                            info: AssetInfo::Token {
                                contract_addr: HumanAddr::from("asset"),
                            },
                            amount: Uint128(1000000u128),
                        },
                        belief_price: None,
                        max_spread: None,
                        to: Some(HumanAddr::from("addr0000")),
                    })
                    .unwrap()
                )
            })
            .unwrap()
        })]
    );
}

#[test]
fn query_buy_with_routes() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        terraswap_factory: HumanAddr("terraswapfactory".to_string()),
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env.clone(), msg).unwrap();

    // set tax rate as 5%
    deps.querier.with_tax(
        Decimal::percent(5),
        &[
            (&"uusd".to_string(), &Uint128(1000000u128)),
            (&"ukrw".to_string(), &Uint128(1000000u128)),
        ],
    );

    let mut _block_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(1000000u128),
        block_time: _block_time,
        operations: vec![
            SwapOperation::NativeSwap {
                offer_denom: "uusd".to_string(),
                ask_denom: "ukrw".to_string(),
            },
            SwapOperation::TerraSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: HumanAddr::from("asset0000"),
                },
            },
            SwapOperation::TerraSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: HumanAddr::from("asset0000"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
        ],
    };

    deps.querier.with_terraswap_pairs(&[
        (&"ukrwasset0000".to_string(), &HumanAddr::from("pair0000")),
        (&"asset0000uluna".to_string(), &HumanAddr::from("pair0001")),
    ]);

    let res: SimulateSwapOperationsResponse = from_binary(&query(&deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        SimulateSwapOperationsResponse {
            amount: Uint128::from(952380u128), // tax charged 1 times uusd => ukrw, ukrw => asset0000, asset0000 => uluna
        }
    );

    _block_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(1000000u128),
        block_time: _block_time,
        operations: vec![
            SwapOperation::NativeSwap {
                offer_denom: "uusd".to_string(),
                ask_denom: "ukrw".to_string(),
            },
            SwapOperation::NativeSwap {
                offer_denom: "ukrw".to_string(),
                ask_denom: "uluna".to_string(),
            },
        ],
    };

    let res: SimulateSwapOperationsResponse = from_binary(&query(&deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        SimulateSwapOperationsResponse {
            amount: Uint128::from(952380u128), // tax charged 1 times uusd => ukrw, ukrw => uluna
        }
    );
}

#[test]
fn assert_minimum_receive_native_token() {
    let mut deps = mock_dependencies(20, &[]);
    deps.querier.with_balance(&[(
        &HumanAddr::from("addr0000"),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(1000000u128),
        }],
    )]);

    let env = mock_env("addr0000", &[]);
    // success
    let msg = HandleMsg::AssertMinimumReceive {
        asset_info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
        prev_balance: Uint128::zero(),
        minimum_receive: Uint128::from(1000000u128),
        receiver: HumanAddr::from("addr0000"),
    };
    let _res = handle(&mut deps, env.clone(), msg).unwrap();

    // assertion failed; native token
    let msg = HandleMsg::AssertMinimumReceive {
        asset_info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
        prev_balance: Uint128::zero(),
        minimum_receive: Uint128::from(1000001u128),
        receiver: HumanAddr::from("addr0000"),
    };
    let res = handle(&mut deps, env.clone(), msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            "assertion failed; minimum receive amount: 1000001, swap amount: 1000000"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }
}

#[test]
fn assert_minimum_receive_token() {
    let mut deps = mock_dependencies(20, &[]);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token0000"),
        &[(&HumanAddr::from("addr0000"), &Uint128::from(1000000u128))],
    )]);

    let env = mock_env("addr0000", &[]);
    // success
    let msg = HandleMsg::AssertMinimumReceive {
        asset_info: AssetInfo::Token {
            contract_addr: HumanAddr::from("token0000"),
        },
        prev_balance: Uint128::zero(),
        minimum_receive: Uint128::from(1000000u128),
        receiver: HumanAddr::from("addr0000"),
    };
    let _res = handle(&mut deps, env.clone(), msg).unwrap();

    // assertion failed; native token
    let msg = HandleMsg::AssertMinimumReceive {
        asset_info: AssetInfo::Token {
            contract_addr: HumanAddr::from("token0000"),
        },
        prev_balance: Uint128::zero(),
        minimum_receive: Uint128::from(1000001u128),
        receiver: HumanAddr::from("addr0000"),
    };
    let res = handle(&mut deps, env.clone(), msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            "assertion failed; minimum receive amount: 1000001, swap amount: 1000000"
        ),
        _ => panic!("DO NOT ENTER HERE"),
    }
}
