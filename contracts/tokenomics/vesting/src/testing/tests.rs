use crate::contract::{handle, init, query};
use terraswap::vesting::{
    ConfigResponse, HandleMsg, InitMsg, QueryMsg, VestingAccount, VestingAccountResponse,
    VestingAccountsResponse, VestingInfo, OrderBy
};

use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{from_binary, log, to_binary, CosmosMsg, HumanAddr, StdError, Uint128, WasmMsg};
use cw20::Cw20HandleMsg;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr::from("owner"),
        token_addr: HumanAddr::from("astro_token"),
        genesis_time: 12345u64,
    };

    let env = mock_env("addr0000", &vec![]);
    let _res = init(&mut deps, env, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(&deps, QueryMsg::Config {}).unwrap()).unwrap(),
        ConfigResponse {
            owner: HumanAddr::from("owner"),
            token_addr: HumanAddr::from("astro_token"),
            genesis_time: 12345u64,
        }
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr::from("owner"),
        token_addr: HumanAddr::from("astro_token"),
        genesis_time: 12345u64,
    };

    let env = mock_env("addr0000", &vec![]);
    let _res = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr::from("owner2")),
        token_addr: None,
        genesis_time: None,
    };
    let env = mock_env("owner", &vec![]);
    let _res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(&deps, QueryMsg::Config {}).unwrap()).unwrap(),
        ConfigResponse {
            owner: HumanAddr::from("owner2"),
            token_addr: HumanAddr::from("astro_token"),
            genesis_time: 12345u64,
        }
    );

    let msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr::from("owner")),
        token_addr: None,
        genesis_time: None,
    };
    let env = mock_env("owner", &vec![]);
    let res = handle(&mut deps, env, msg);
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    let msg = HandleMsg::UpdateConfig {
        owner: None,
        token_addr: Some(HumanAddr::from("token_addr2")),
        genesis_time: Some(1u64),
    };
    let env = mock_env("owner2", &vec![]);
    let _res = handle(&mut deps, env, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(&deps, QueryMsg::Config {}).unwrap()).unwrap(),
        ConfigResponse {
            owner: HumanAddr::from("owner2"),
            token_addr: HumanAddr::from("token_addr2"),
            genesis_time: 1u64,
        }
    );
}

#[test]
fn register_vesting_accounts() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr::from("owner"),
        token_addr: HumanAddr::from("astro_token"),
        genesis_time: 100u64,
    };

    let env = mock_env("addr0000", &vec![]);
    let _res = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::RegisterVestingAccounts {
        vesting_accounts: vec![
            VestingAccount {
                address: HumanAddr::from("addr0000"),
                schedules: vec![
                    (100u64, 101u64, Uint128::from(100u128)),
                    (100u64, 110u64, Uint128::from(100u128)),
                    (100u64, 200u64, Uint128::from(100u128)),
                ],
            },
            VestingAccount {
                address: HumanAddr::from("addr0001"),
                schedules: vec![(100u64, 110u64, Uint128::from(100u128))],
            },
            VestingAccount {
                address: HumanAddr::from("addr0002"),
                schedules: vec![(100u64, 200u64, Uint128::from(100u128))],
            },
        ],
    };
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    let env = mock_env("owner", &[]);
    let _res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        from_binary::<VestingAccountResponse>(
            &query(
                &deps,
                QueryMsg::VestingAccount {
                    address: HumanAddr::from("addr0000"),
                }
            )
            .unwrap()
        )
        .unwrap(),
        VestingAccountResponse {
            address: HumanAddr::from("addr0000"),
            info: VestingInfo {
                last_claim_time: 100u64,
                schedules: vec![
                    (100u64, 101u64, Uint128::from(100u128)),
                    (100u64, 110u64, Uint128::from(100u128)),
                    (100u64, 200u64, Uint128::from(100u128)),
                ],
            }
        }
    );

    assert_eq!(
        from_binary::<VestingAccountsResponse>(
            &query(
                &deps,
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
                    address: HumanAddr::from("addr0000"),
                    info: VestingInfo {
                        last_claim_time: 100u64,
                        schedules: vec![
                            (100u64, 101u64, Uint128::from(100u128)),
                            (100u64, 110u64, Uint128::from(100u128)),
                            (100u64, 200u64, Uint128::from(100u128)),
                        ],
                    }
                },
                VestingAccountResponse {
                    address: HumanAddr::from("addr0001"),
                    info: VestingInfo {
                        last_claim_time: 100u64,
                        schedules: vec![(100u64, 110u64, Uint128::from(100u128))],
                    }
                },
                VestingAccountResponse {
                    address: HumanAddr::from("addr0002"),
                    info: VestingInfo {
                        last_claim_time: 100u64,
                        schedules: vec![(100u64, 200u64, Uint128::from(100u128))],
                    }
                }
            ]
        }
    );
}

#[test]
fn claim() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr::from("owner"),
        token_addr: HumanAddr::from("astro_token"),
        genesis_time: 100u64,
    };

    let env = mock_env("addr0000", &vec![]);
    let _res = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: HumanAddr::from("addr0000"),
            schedules: vec![
                (100u64, 101u64, Uint128::from(100u128)),
                (100u64, 110u64, Uint128::from(100u128)),
                (100u64, 200u64, Uint128::from(100u128)),
            ],
        }],
    };
    let env = mock_env("owner", &[]);
    let _res = handle(&mut deps, env, msg.clone()).unwrap();

    let mut env = mock_env("addr0000", &[]);
    env.block.time = 100;

    let msg = HandleMsg::Claim {};
    let res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "claim"),
            log("address", "addr0000"),
            log("claim_amount", "0"),
            log("last_claim_time", "100"),
        ]
    );
    assert_eq!(res.messages, vec![],);

    env.block.time = 101;
    let res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "claim"),
            log("address", "addr0000"),
            log("claim_amount", "111"),
            log("last_claim_time", "101"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("astro_token"),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from("addr0000"),
                amount: Uint128::from(111u128),
            })
            .unwrap(),
            send: vec![],
        })],
    );

    env.block.time = 102;
    let res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "claim"),
            log("address", "addr0000"),
            log("claim_amount", "11"),
            log("last_claim_time", "102"),
        ]
    );
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("astro_token"),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from("addr0000"),
                amount: Uint128::from(11u128),
            })
            .unwrap(),
            send: vec![],
        })],
    );
}
