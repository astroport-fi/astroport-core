use crate::contract::{execute, instantiate, query};
use astroport::vesting::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, OrderBy, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
    VestingSchedulePoint,
};

use crate::error::ContractError;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, ReplyOn, SubMsg, Timestamp, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            token_addr: Addr::unchecked("astro_token"),
        }
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("owner2".to_string()),
    };

    let info = mock_info("owner", &vec![]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner2"),
            token_addr: Addr::unchecked("astro_token"),
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("owner".to_string()),
    };

    let env = mock_env();
    let info = mock_info("owner", &vec![]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});
}

#[test]
fn register_vesting_accounts() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![
            VestingAccount {
                address: "addr0000".to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200),
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            },
            VestingAccount {
                address: "addr0001".to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(110),
                        amount: Uint128::new(100),
                    }),
                }],
            },
            VestingAccount {
                address: "addr0002".to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(200),
                        amount: Uint128::new(100),
                    }),
                }],
            },
        ],
    };
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        from_binary::<VestingAccountResponse>(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::VestingAccount {
                    address: Addr::unchecked("addr0000"),
                }
            )
            .unwrap()
        )
        .unwrap(),
        VestingAccountResponse {
            address: Addr::unchecked("addr0000"),
            info: VestingInfo {
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero()
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101),
                            amount: Uint128::new(100)
                        },)
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero()
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110),
                            amount: Uint128::new(100)
                        },)
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100),
                            amount: Uint128::zero()
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200),
                            amount: Uint128::new(100)
                        },)
                    },
                ],
                released_amount: Uint128::zero()
            }
        }
    );

    assert_eq!(
        from_binary::<VestingAccountsResponse>(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::VestingAccounts {
                    limit: None,
                    start_after: None,
                    order_by: Some(OrderBy::Asc),
                }
            )
            .unwrap()
        )
        .unwrap(),
        VestingAccountsResponse {
            vesting_accounts: vec![
                VestingAccountResponse {
                    address: Addr::unchecked("addr0000"),
                    info: VestingInfo {
                        schedules: vec![
                            VestingSchedule {
                                start_point: VestingSchedulePoint {
                                    time: Timestamp::from_seconds(100),
                                    amount: Uint128::zero()
                                },
                                end_point: Some(VestingSchedulePoint {
                                    time: Timestamp::from_seconds(101),
                                    amount: Uint128::new(100)
                                },)
                            },
                            VestingSchedule {
                                start_point: VestingSchedulePoint {
                                    time: Timestamp::from_seconds(100),
                                    amount: Uint128::zero()
                                },
                                end_point: Some(VestingSchedulePoint {
                                    time: Timestamp::from_seconds(110),
                                    amount: Uint128::new(100)
                                },)
                            },
                            VestingSchedule {
                                start_point: VestingSchedulePoint {
                                    time: Timestamp::from_seconds(100),
                                    amount: Uint128::zero()
                                },
                                end_point: Some(VestingSchedulePoint {
                                    time: Timestamp::from_seconds(200),
                                    amount: Uint128::new(100)
                                },)
                            },
                        ],
                        released_amount: Uint128::zero()
                    }
                },
                VestingAccountResponse {
                    address: Addr::unchecked("addr0001"),
                    info: VestingInfo {
                        schedules: vec![VestingSchedule {
                            start_point: VestingSchedulePoint {
                                time: Timestamp::from_seconds(100),
                                amount: Uint128::zero()
                            },
                            end_point: Some(VestingSchedulePoint {
                                time: Timestamp::from_seconds(110),
                                amount: Uint128::new(100)
                            },)
                        },],
                        released_amount: Uint128::zero()
                    }
                },
                VestingAccountResponse {
                    address: Addr::unchecked("addr0002"),
                    info: VestingInfo {
                        schedules: vec![VestingSchedule {
                            start_point: VestingSchedulePoint {
                                time: Timestamp::from_seconds(100),
                                amount: Uint128::zero()
                            },
                            end_point: Some(VestingSchedulePoint {
                                time: Timestamp::from_seconds(200),
                                amount: Uint128::new(100)
                            },)
                        },],
                        released_amount: Uint128::zero()
                    }
                }
            ]
        }
    );
}

#[test]
fn claim() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let mut env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: "addr0000".to_string(),
            schedules: vec![
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(101),
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(110),
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(200),
                        amount: Uint128::new(100),
                    }),
                },
            ],
        }],
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    let info = mock_info("addr0000", &[]);
    env.block.time = Timestamp::from_seconds(100);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.events[0].attributes,
        vec![
            attr("address", "addr0000"),
            attr("available_amount", "0"),
            attr("claimed_amount", "0"),
        ]
    );
    assert_eq!(res.messages, vec![],);

    env.block.time = Timestamp::from_seconds(101);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.events[0].attributes,
        vec![
            attr("address", "addr0000"),
            attr("available_amount", "111"),
            attr("claimed_amount", "111"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("astro_token"),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::new(111u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }],
    );

    env.block.time = Timestamp::from_seconds(102);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.events[0].attributes,
        vec![
            attr("address", "addr0000"),
            attr("available_amount", "11"),
            attr("claimed_amount", "11"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("astro_token"),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::new(11u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }],
    );
}

#[test]
fn vesting_account_available_amount() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let mut env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: "addr0000".to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(101),
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    env.block.time = Timestamp::from_seconds(100);

    let query_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::AvailableAmount {
            address: Addr::unchecked("addr0000"),
        },
    )
    .unwrap();

    let vesting_res: Uint128 = from_binary(&query_res).unwrap();
    assert_eq!(vesting_res, Uint128::new(0u128));
}
