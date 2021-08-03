use crate::contract::{execute, instantiate, query};
use terraswap::vesting::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, OrderBy, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo,
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
        owner: Addr::unchecked("owner"),
        token_addr: Addr::unchecked("astro_token"),
        genesis_time: Timestamp::from_seconds(12345),
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
            genesis_time: Timestamp::from_seconds(12345),
        }
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: Addr::unchecked("owner"),
        token_addr: Addr::unchecked("astro_token"),
        genesis_time: Timestamp::from_seconds(12345),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(Addr::unchecked("owner2")),
        token_addr: None,
        genesis_time: None,
    };

    let info = mock_info("owner", &vec![]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner2"),
            token_addr: Addr::unchecked("astro_token"),
            genesis_time: Timestamp::from_seconds(12345),
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(Addr::unchecked("owner")),
        token_addr: None,
        genesis_time: None,
    };

    let env = mock_env();
    let info = mock_info("owner", &vec![]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    let msg = ExecuteMsg::UpdateConfig {
        owner: None,
        token_addr: Some(Addr::unchecked("token_addr2")),
        genesis_time: Some(Timestamp::from_seconds(1u64)),
    };
    let info = mock_info("owner2", &vec![]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(
            &query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()
        )
        .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner2"),
            token_addr: Addr::unchecked("token_addr2"),
            genesis_time: Timestamp::from_seconds(1),
        }
    );
}

#[test]
fn register_vesting_accounts() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: Addr::unchecked("owner"),
        token_addr: Addr::unchecked("astro_token"),
        genesis_time: Timestamp::from_seconds(100),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![
            VestingAccount {
                address: Addr::unchecked("addr0000"),
                schedules: vec![
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(101),
                        Uint128::from(100u128),
                    ),
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(110),
                        Uint128::from(100u128),
                    ),
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(200),
                        Uint128::from(100u128),
                    ),
                ],
            },
            VestingAccount {
                address: Addr::unchecked("addr0001"),
                schedules: vec![(
                    Timestamp::from_seconds(100),
                    Timestamp::from_seconds(110),
                    Uint128::from(100u128),
                )],
            },
            VestingAccount {
                address: Addr::unchecked("addr0002"),
                schedules: vec![(
                    Timestamp::from_seconds(100),
                    Timestamp::from_seconds(200),
                    Uint128::from(100u128),
                )],
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
                last_claim_time: Timestamp::from_seconds(100),
                schedules: vec![
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(101),
                        Uint128::from(100u128)
                    ),
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(110),
                        Uint128::from(100u128)
                    ),
                    (
                        Timestamp::from_seconds(100),
                        Timestamp::from_seconds(200),
                        Uint128::from(100u128)
                    ),
                ],
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
                        last_claim_time: Timestamp::from_seconds(100),
                        schedules: vec![
                            (
                                Timestamp::from_seconds(100),
                                Timestamp::from_seconds(101),
                                Uint128::from(100u128)
                            ),
                            (
                                Timestamp::from_seconds(100),
                                Timestamp::from_seconds(110),
                                Uint128::from(100u128)
                            ),
                            (
                                Timestamp::from_seconds(100),
                                Timestamp::from_seconds(200),
                                Uint128::from(100u128)
                            ),
                        ],
                    }
                },
                VestingAccountResponse {
                    address: Addr::unchecked("addr0001"),
                    info: VestingInfo {
                        last_claim_time: Timestamp::from_seconds(100),
                        schedules: vec![(
                            Timestamp::from_seconds(100),
                            Timestamp::from_seconds(110),
                            Uint128::from(100u128)
                        )],
                    }
                },
                VestingAccountResponse {
                    address: Addr::unchecked("addr0002"),
                    info: VestingInfo {
                        last_claim_time: Timestamp::from_seconds(100),
                        schedules: vec![(
                            Timestamp::from_seconds(100),
                            Timestamp::from_seconds(200),
                            Uint128::from(100u128)
                        )],
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
        owner: Addr::unchecked("owner"),
        token_addr: Addr::unchecked("astro_token"),
        genesis_time: Timestamp::from_seconds(100),
    };

    let mut env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: Addr::unchecked("addr0000"),
            schedules: vec![
                (
                    Timestamp::from_seconds(100),
                    Timestamp::from_seconds(101),
                    Uint128::from(100u128),
                ),
                (
                    Timestamp::from_seconds(100),
                    Timestamp::from_seconds(110),
                    Uint128::from(100u128),
                ),
                (
                    Timestamp::from_seconds(100),
                    Timestamp::from_seconds(200),
                    Uint128::from(100u128),
                ),
            ],
        }],
    };

    let info = mock_info("owner", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    let info = mock_info("addr0000", &[]);
    env.block.time = Timestamp::from_seconds(100);

    let msg = ExecuteMsg::Claim {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "claim"),
            attr("address", "addr0000"),
            attr("claim_amount", "0"),
            attr("last_claim_time", "100"),
        ]
    );
    assert_eq!(res.messages, vec![],);

    env.block.time = Timestamp::from_seconds(101);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "claim"),
            attr("address", "addr0000"),
            attr("claim_amount", "111"),
            attr("last_claim_time", "101"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("astro_token"),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(111u128),
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
        res.attributes,
        vec![
            attr("action", "claim"),
            attr("address", "addr0000"),
            attr("claim_amount", "11"),
            attr("last_claim_time", "102"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("astro_token"),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(11u128),
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
