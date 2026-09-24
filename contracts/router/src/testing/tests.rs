use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_json, to_json_binary, Addr, Coin, ReplyOn, SubMsg, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport::asset::{native_asset_info, AssetInfo};
use astroport::router::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
    SimulateSwapOperationsResponse, SwapOperation, MAX_SWAP_OPERATIONS,
};

use crate::contract::{execute, instantiate, query, AFTER_SWAP_REPLY_ID};
use crate::error::ContractError;
use crate::testing::mock_querier::mock_dependencies;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        astroport_factory: String::from("astroportfactory"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // It worked, let's query the state
    let config: ConfigResponse =
        from_json(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!("astroportfactory", config.astroport_factory.as_str());
}

#[test]
fn execute_swap_operations() {
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {
        astroport_factory: String::from("astroportfactory"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![],
        minimum_receive: None,
        to: None,
        max_spread: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::MustProvideOperations {});

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0002"),
                },
            },
        ],
        minimum_receive: Some(Uint128::from(1000000u128)),
        to: None,
        max_spread: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken {
                                denom: "ukrw".to_string(),
                            },
                            ask_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0001"),
                            },
                        },
                        to: None,
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            },
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0001"),
                            },
                            ask_asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                        },
                        to: None,
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            },
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            ask_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0002"),
                            },
                        },
                        to: Some(String::from("addr0000")),
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: AFTER_SWAP_REPLY_ID,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            }
        ]
    );

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: Uint128::from(1000000u128),
        msg: to_json_binary(&Cw20HookMsg::ExecuteSwapOperations {
            operations: vec![
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "ukrw".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0002"),
                    },
                },
            ],
            minimum_receive: None,
            to: Some(String::from("addr0002")),
            max_spread: None,
        })
        .unwrap(),
    });

    // mock deps don't run the final reply, so finish the previous route by hand
    crate::state::REPLY_DATA.remove(deps.as_mut().storage);

    let env = mock_env();
    let info = mock_info("asset0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken {
                                denom: "ukrw".to_string(),
                            },
                            ask_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0001"),
                            },
                        },
                        to: None,
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            },
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0001"),
                            },
                            ask_asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                        },
                        to: None,
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            },
            SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    funds: vec![],
                    msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperation {
                        operation: SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            ask_asset_info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("asset0002"),
                            },
                        },
                        to: Some(String::from("addr0002")),
                        max_spread: None,
                        single: false
                    })
                    .unwrap(),
                }
                .into(),
                id: AFTER_SWAP_REPLY_ID,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            }
        ]
    );
}

#[test]
fn execute_swap_operation() {
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {
        astroport_factory: String::from("astroportfactory"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    deps.querier
        .with_astroport_pairs(&[(&"uusdasset".to_string(), &String::from("pair"))]);
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            amount: Uint128::new(1000000u128),
            denom: "uusd".to_string(),
        }],
    )]);

    deps.querier
        .with_astroport_pairs(&[(&"assetuusd".to_string(), &String::from("pair"))]);
    deps.querier.with_token_balances(&[(
        &String::from("asset"),
        &[(
            &String::from(MOCK_CONTRACT_ADDR),
            &Uint128::new(1000000u128),
        )],
    )]);
    let msg = ExecuteMsg::ExecuteSwapOperation {
        operation: SwapOperation::AstroSwap {
            offer_asset_info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset"),
            },
            ask_asset_info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        to: Some(String::from("addr0000")),
        max_spread: None,
        single: true,
    };
    let env = mock_env();
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset"),
                funds: vec![],
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: String::from("pair"),
                    amount: Uint128::new(1000000u128),
                    msg: to_json_binary(&astroport::pair::Cw20HookMsg::Swap {
                        ask_asset_info: Some(native_asset_info("uusd".to_string())),
                        belief_price: None,
                        max_spread: None,
                        to: Some(String::from("addr0000")),
                    })
                    .unwrap()
                })
                .unwrap()
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }]
    );
}

#[test]
fn query_buy_with_routes() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        astroport_factory: String::from("astroportfactory"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(1000000u128),
        operations: vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
        ],
    };
    deps.querier.with_astroport_pairs(&[
        (&"ukrwasset0000".to_string(), &String::from("pair0000")),
        (&"asset0000uluna".to_string(), &String::from("pair0001")),
    ]);

    let res: SimulateSwapOperationsResponse =
        from_json(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        SimulateSwapOperationsResponse {
            amount: Uint128::from(1000000u128)
        }
    );

    assert_eq!(
        res,
        SimulateSwapOperationsResponse {
            amount: Uint128::from(1000000u128),
        }
    );

    let msg = QueryMsg::SimulateSwapOperations {
        offer_amount: Uint128::from(1000000u128),
        operations: vec![SwapOperation::NativeSwap {
            offer_denom: "ukrw".to_string(),
            ask_denom: "test".to_string(),
        }],
    };
    let err = query(deps.as_ref(), env.clone(), msg).unwrap_err();
    assert_eq!(err, ContractError::NativeSwapNotSupported {});
}

#[test]
fn assert_maximum_receive_swap_operations() {
    let mut deps = mock_dependencies(&[]);
    let msg = InstantiateMsg {
        astroport_factory: String::from("astroportfactory"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::NativeSwap {
                offer_denom: "uusd".to_string(),
                ask_denom: "ukrw".to_string(),
            };
            MAX_SWAP_OPERATIONS + 1
        ],
        minimum_receive: None,
        to: None,
        max_spread: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();

    assert_eq!(res, ContractError::SwapLimitExceeded {});
}

#[test]
fn refuses_a_route_while_another_is_in_progress() {
    use crate::state::{ReplyData, REPLY_DATA};

    let mut deps = mock_dependencies(&[]);
    instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info("addr0000", &[]),
        InstantiateMsg {
            astroport_factory: String::from("astroportfactory"),
        },
    )
    .unwrap();

    // as if a route had started and not reached its final reply yet
    REPLY_DATA
        .save(
            deps.as_mut().storage,
            &ReplyData {
                asset_info: native_asset_info("uluna".to_string()),
                prev_balance: Uint128::zero(),
                minimum_receive: Some(Uint128::new(1)),
                receiver: String::from("addr0000"),
            },
        )
        .unwrap();

    let msg = ExecuteMsg::ExecuteSwapOperations {
        operations: vec![SwapOperation::AstroSwap {
            offer_asset_info: native_asset_info("ukrw".to_string()),
            ask_asset_info: native_asset_info("uluna".to_string()),
        }],
        minimum_receive: None,
        to: None,
        max_spread: None,
    };
    let err = execute(deps.as_mut(), mock_env(), mock_info("addr0000", &[]), msg).unwrap_err();
    assert_eq!(err, ContractError::RouteInProgress {});
}

#[test]
fn migration_clears_leftover_route_data() {
    use crate::contract::migrate;
    use crate::state::{ReplyData, REPLY_DATA};
    use cosmwasm_std::Empty;

    let mut deps = mock_dependencies(&[]);
    instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info("addr0000", &[]),
        InstantiateMsg {
            astroport_factory: String::from("astroportfactory"),
        },
    )
    .unwrap();

    // 1.3.1 never cleared the last route's data
    cw2::set_contract_version(deps.as_mut().storage, "astroport-router", "1.3.1").unwrap();
    REPLY_DATA
        .save(
            deps.as_mut().storage,
            &ReplyData {
                asset_info: native_asset_info("uluna".to_string()),
                prev_balance: Uint128::zero(),
                minimum_receive: None,
                receiver: String::from("addr0000"),
            },
        )
        .unwrap();

    migrate(deps.as_mut(), mock_env(), Empty {}).unwrap();

    assert!(!REPLY_DATA.exists(deps.as_ref().storage));
}
