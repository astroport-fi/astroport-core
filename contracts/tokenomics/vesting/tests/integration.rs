use astroport::asset::{native_asset_info, token_asset_info};
use astroport::querier::query_balance;
use astroport::vesting::{QueryMsg, VestingAccountResponse};
use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg, ExecuteMsg, InstantiateMsg, VestingAccount, VestingSchedule,
        VestingSchedulePoint,
    },
};
use astroport_vesting::error::ContractError;
use astroport_vesting::state::Config;
use cosmwasm_std::{coin, coins, to_binary, Addr, StdResult, Timestamp, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};
use cw_utils::PaymentError;

const OWNER1: &str = "owner1";
const USER1: &str = "user1";
const USER2: &str = "user2";
const TOKEN_INITIAL_AMOUNT: u128 = 1_000_000_000_000000;
const IBC_ASTRO: &str = "ibc/ASTRO-TOKEN";

#[test]
fn claim() {
    let user1 = Addr::unchecked(USER1);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let vesting_instance = instantiate_vesting(&mut app, &astro_token_instance);

    let current_time = app.block_info().time.seconds();

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 101,
                            amount: Uint128::new(200),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 110,
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 200,
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    let res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        res.root_cause().to_string(),
        "Vesting schedule amount error. The total amount should be equal to the received amount."
    );

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 101,
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 110,
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: current_time + 100,
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: current_time + 200,
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    app.execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(200);
        b.height += 200 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(300u128));

    // Check owner balance
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    // Check vesting balance
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        300u128,
    );

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(300u128));

    // Check vesting balance
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        0u128,
    );

    // Check user balance
    check_token_balance(&mut app, &astro_token_instance, &user1.clone(), 300u128);

    // Owner balance mustn't change after claim
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    // Check user balance after claim
    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(0u128));
}

#[test]
fn claim_native() {
    let user1 = Addr::unchecked(USER1);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let random_token_instance =
        instantiate_token(&mut app, token_code_id, "RND", Some(1_000_000000));

    mint_tokens(&mut app, &random_token_instance, &owner, 1_000_000000);

    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let current_time = app.block_info().time.seconds();

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 101,
                        amount: Uint128::new(200),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    let err = app
        .execute_contract(owner.clone(), random_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 101,
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 110,
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 200,
                        amount: Uint128::new(100),
                    }),
                },
            ],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(300, IBC_ASTRO),
    )
    .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(200);
        b.height += 200 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(300u128));

    // Check owner balance
    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);

    // Check vesting balance
    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 300u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    app.execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(
            vesting_instance.clone(),
            &QueryMsg::VestingAccount {
                address: user1.to_string(),
            },
        )
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(300u128));

    // Check vesting balance
    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 0);

    // Check user balance
    let bal = query_balance(&app.wrap(), &user1, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 300);

    // Owner balance mustn't change after claim
    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    // Check user balance after claim
    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(0u128));
}

#[test]
fn register_vesting_accounts() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let noname_token_instance = instantiate_token(
        &mut app,
        token_code_id,
        "NONAME",
        Some(1_000_000_000_000000),
    );

    mint_tokens(
        &mut app,
        &noname_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    let vesting_instance = instantiate_vesting(&mut app, &astro_token_instance);

    let current_time = app.block_info().time.seconds();

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 150,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Vesting schedule error on addr: user1. Should satisfy: (start < end, end > current_time and start_amount < end_amount)");

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 150,
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = app
        .execute_contract(
            user1.clone(),
            astro_token_instance.clone(),
            &msg.clone(),
            &[],
        )
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Cannot Sub with 0 and 100");

    let res = app
        .execute_contract(owner.clone(), noname_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Unauthorized");

    // Checking that execute endpoint with native coin is unreachable if ASTRO is a cw20 token
    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: current_time + 100,
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: current_time + 150,
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let err = app
        .execute_contract(
            owner.clone(),
            vesting_instance.clone(),
            &native_msg,
            &coins(100u128, "random-coin"),
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let _res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(150);
        b.height += 150 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(100u128));
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 100u128,
    );
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        100u128,
    );

    let current_time = app.block_info().time.seconds();

    // Let's check user1's final vesting amount after add schedule for a new one
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user2.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 150,
                        amount: Uint128::new(200),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(200u128),
    };

    let _res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(150);
        b.height += 150 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user2.to_string(),
    };

    let user2_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        300u128,
    );
    // A new schedule has been added successfully and an old one hasn't changed.
    // The new schedule doesn't have the same value as the old one.
    assert_eq!(user2_vesting_amount, Uint128::new(200u128));
    assert_eq!(user1_vesting_amount, Uint128::from(100u128));

    let current_time = app.block_info().time.seconds();

    // Add one more vesting schedule; final amount to vest must increase
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 200,
                        amount: Uint128::new(10),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(10u128),
    };

    let _res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(200);
        b.height += 200 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(vesting_res, Uint128::new(110u128));
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        310u128,
    );

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(110u128));
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &vesting_instance.clone(),
        200u128,
    );
    check_token_balance(&mut app, &astro_token_instance, &user1.clone(), 110u128);

    // Owner balance mustn't change after claim
    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
}

#[test]
fn register_vesting_accounts_native() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let random_token_instance =
        instantiate_token(&mut app, token_code_id, "RND", Some(1_000_000_000_000000));

    mint_tokens(
        &mut app,
        &random_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let current_time = app.block_info().time.seconds();

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_time + 100,
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: current_time + 150,
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let err = app
        .execute_contract(owner.clone(), random_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    // Checking that execute endpoint with random native coin is unreachable
    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: current_time + 100,
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: current_time + 150,
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let err = app
        .execute_contract(
            owner.clone(),
            vesting_instance.clone(),
            &native_msg,
            &coins(100u128, "random-coin"),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::PaymentError(PaymentError::MissingDenom("ibc/ASTRO-TOKEN".to_string())),
        err.downcast().unwrap()
    );

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &native_msg,
        &coins(100u128, IBC_ASTRO),
    )
    .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(150);
        b.height += 150 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.u128(), 100u128);

    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 100u128);

    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 100);

    let current_time = app.block_info().time.seconds();

    // Let's check user1's final vesting amount after add schedule for a new one
    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user2.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: current_time + 100,
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: current_time + 150,
                    amount: Uint128::new(200),
                }),
            }],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(200, IBC_ASTRO),
    )
    .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(150);
        b.height += 150 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user2.to_string(),
    };

    let user2_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);
    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 300u128);

    // A new schedule has been added successfully and an old one hasn't changed.
    // The new schedule doesn't have the same value as the old one.
    assert_eq!(user2_vesting_amount, Uint128::new(200u128));
    assert_eq!(user1_vesting_amount, Uint128::from(100u128));

    let current_time = app.block_info().time.seconds();

    // Add one more vesting schedule; final amount to vest must increase
    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: current_time + 100,
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: current_time + 200,
                    amount: Uint128::new(10),
                }),
            }],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(10, IBC_ASTRO),
    )
    .unwrap();

    app.update_block(|b| {
        b.time = b.time.plus_seconds(200);
        b.height += 200 / 5
    });

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res, Uint128::new(110u128));

    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 310u128);
    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 310u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(110u128));

    let bal = query_balance(&app.wrap(), &vesting_instance, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 200);
    let bal = query_balance(&app.wrap(), &user1, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, 110u128);

    let bal = query_balance(&app.wrap(), &owner, IBC_ASTRO)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 310u128);
}

#[test]
fn withdraw_from_active_schedule() {
    let owner = Addr::unchecked(OWNER1);
    let mut app = mock_app(&owner);
    let token_code_id = store_token_code(&mut app);
    let astro_token = instantiate_token(&mut app, token_code_id, "Astro", None);
    let vesting_instance = instantiate_vesting(&mut app, &astro_token);

    let user1 = Addr::unchecked("user1");
    let vested_amount = Uint128::new(100_000_000_000000);
    let start_time = 1654599600;
    let end_time = 1686135600;
    let now_ts = 1675159485;

    app.update_block(|b| b.time = Timestamp::from_seconds(start_time));

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: start_time,
                        amount: Uint128::new(1_000_000_000000),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: end_time,
                        amount: Uint128::new(100_000_000_000000),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: vested_amount,
    };
    app.execute_contract(owner.clone(), astro_token.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| b.time = Timestamp::from_seconds(now_ts));

    // Claim and check current amount
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        65_543_017_979452,
    );

    let withdraw_amount = Uint128::new(10_000_000_000000);
    let recipient = Addr::unchecked("recipient");
    let withdraw_msg = ExecuteMsg::WithdrawFromActiveSchedule {
        account: user1.to_string(),
        recipient: Some(recipient.to_string()),
        withdraw_amount,
    };
    app.execute_contract(owner.clone(), vesting_instance.clone(), &withdraw_msg, &[])
        .unwrap();

    // Recipient received tokens
    let recipient_bal = query_token_balance(&mut app, &astro_token, &recipient);
    assert_eq!(recipient_bal, withdraw_amount);

    // User1 did not receive tokens after withdraw event
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        65_543_017_979452,
    );

    app.update_block(|b| b.time = b.time.plus_seconds(86400 * 7));

    // User1 available amount is still being increased but now with reduced slope
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        66_890_633_481478,
    );

    app.update_block(|b| b.time = Timestamp::from_seconds(end_time));

    // In the end of the schedule user1 receives all tokens minus withdrawn amount
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        (vested_amount - withdraw_amount).u128(),
    );
}

#[test]
fn withdraw_overlapping_schedules() {
    let owner = Addr::unchecked(OWNER1);
    let mut app = mock_app(&owner);
    let token_code_id = store_token_code(&mut app);
    let astro_token = instantiate_token(&mut app, token_code_id, "Astro", None);
    let vesting_instance = instantiate_vesting(&mut app, &astro_token);

    let user1 = Addr::unchecked("user1");
    let vested_amount = Uint128::new(100_000_000_000000);
    let start_time = 1654599600;
    let end_time = 1686135600;
    let now_ts = 1675159485;

    app.update_block(|b| b.time = Timestamp::from_seconds(start_time));

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: start_time,
                            amount: Uint128::new(1_000_000_000000),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: end_time,
                            amount: Uint128::new(50_000_000_000000),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: now_ts - 86400 * 7,
                            amount: Uint128::new(50_000_000_000000),
                        },
                        end_point: None,
                    },
                ],
            }],
        })
        .unwrap(),
        amount: vested_amount,
    };
    app.execute_contract(owner.clone(), astro_token.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| b.time = Timestamp::from_seconds(now_ts));

    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        82_945_534_151445,
    );

    let withdraw_amount = Uint128::new(10_000_000_000000);
    let recipient = Addr::unchecked("recipient");
    let withdraw_msg = ExecuteMsg::WithdrawFromActiveSchedule {
        account: user1.to_string(),
        recipient: Some(recipient.to_string()),
        withdraw_amount,
    };

    // Since we do not consider schedule without end point as active it is possible to withdraw from
    // active schedule with end_point.
    app.execute_contract(owner.clone(), vesting_instance.clone(), &withdraw_msg, &[])
        .unwrap();

    // Recipient received tokens
    let recipient_bal = query_token_balance(&mut app, &astro_token, &recipient);
    assert_eq!(recipient_bal, withdraw_amount);

    // User1 did not receive tokens after withdraw event
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        82_945_534_151445,
    );

    // Go to the end of the 1st schedule
    app.update_block(|b| b.time = Timestamp::from_seconds(end_time));

    // In the end of the schedule user1 receives all tokens minus withdrawn amount
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        (vested_amount - withdraw_amount).u128(),
    );
}

#[test]
fn withdraw_overlapping_schedules2() {
    let owner = Addr::unchecked(OWNER1);
    let mut app = mock_app(&owner);
    let token_code_id = store_token_code(&mut app);
    let astro_token = instantiate_token(&mut app, token_code_id, "Astro", None);
    let vesting_instance = instantiate_vesting(&mut app, &astro_token);

    let user1 = Addr::unchecked("user1");
    let vested_amount = Uint128::new(100_000_000_000000);
    let start_time = 1654599600;
    let end_time = 1686135600;
    let now_ts = 1675159485;

    app.update_block(|b| b.time = Timestamp::from_seconds(start_time));

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: start_time,
                            amount: Uint128::new(1_000_000_000000),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: end_time,
                            amount: Uint128::new(50_000_000_000000),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: now_ts - 86400 * 7,
                            amount: Uint128::new(1_000_000_000000),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: end_time + 86400 * 7,
                            amount: Uint128::new(50_000_000_000000),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: vested_amount,
    };
    app.execute_contract(owner.clone(), astro_token.clone(), &msg, &[])
        .unwrap();

    app.update_block(|b| b.time = Timestamp::from_seconds(now_ts));

    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        36_377_496_494237,
    );

    let recipient = Addr::unchecked("recipient");
    let withdraw_msg = ExecuteMsg::WithdrawFromActiveSchedule {
        account: user1.to_string(),
        recipient: Some(recipient.to_string()),
        withdraw_amount: Uint128::new(10_000_000_000000),
    };
    let err = app
        .execute_contract(owner.clone(), vesting_instance.clone(), &withdraw_msg, &[])
        .unwrap_err();
    assert_eq!(
        ContractError::MultipleActiveSchedules(user1.to_string()),
        err.downcast().unwrap(),
    );

    // Go to the end of the 1st schedule
    app.update_block(|b| b.time = Timestamp::from_seconds(end_time));

    // Trying to withdraw again
    let err = app
        .execute_contract(owner.clone(), vesting_instance.clone(), &withdraw_msg, &[])
        .unwrap_err();
    // There is no 10M ASTRO available for withdrawal
    assert_eq!(
        ContractError::NotEnoughTokens(Uint128::new(2_431_962_342793)),
        err.downcast().unwrap(),
    );

    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        97_568_037_657_207,
    );

    // Withdrawing 1M ASTRO
    let withdraw_amount = Uint128::new(1_000_000_000000);
    let withdraw_msg = ExecuteMsg::WithdrawFromActiveSchedule {
        account: user1.to_string(),
        recipient: Some(recipient.to_string()),
        withdraw_amount,
    };
    app.execute_contract(owner.clone(), vesting_instance.clone(), &withdraw_msg, &[])
        .unwrap();

    // Recipient received tokens
    let recipient_bal = query_token_balance(&mut app, &astro_token, &recipient);
    assert_eq!(recipient_bal, withdraw_amount);

    // user1's amount was not changed
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        97_568_037_657_207,
    );

    // Go to the end of the 2nd schedule
    app.update_block(|b| b.time = Timestamp::from_seconds(end_time + 86400 * 7));

    // user1 received all tokens except 1M ASTRO
    claim_and_check(
        &mut app,
        &user1,
        &vesting_instance,
        &astro_token,
        (vested_amount - withdraw_amount).u128(),
    );
}

fn mock_app(owner: &Addr) -> App {
    App::new(|app, _, storage| {
        app.bank
            .init_balance(
                storage,
                owner,
                vec![
                    coin(TOKEN_INITIAL_AMOUNT, IBC_ASTRO),
                    coin(1_000_0000000u128, "random-coin"),
                ],
            )
            .unwrap()
    })
}

fn store_token_code(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn instantiate_token(app: &mut App, token_code_id: u64, name: &str, cap: Option<u128>) -> Addr {
    let name = String::from(name);

    let msg = TokenInstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: String::from(OWNER1),
            cap: cap.map(|v| Uint128::from(v)),
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        Addr::unchecked(OWNER1),
        &msg,
        &[],
        name,
        None,
    )
    .unwrap()
}

fn instantiate_vesting(mut app: &mut App, astro_token_instance: &Addr) -> Addr {
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let owner = Addr::unchecked(OWNER1);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = InstantiateMsg {
        owner: OWNER1.to_string(),
        vesting_token: token_asset_info(astro_token_instance.clone()),
    };

    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Vesting",
            None,
        )
        .unwrap();

    let res: Config = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        astro_token_instance.to_string(),
        res.vesting_token.to_string()
    );

    mint_tokens(
        &mut app,
        &astro_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    check_token_balance(
        &mut app,
        &astro_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    vesting_instance
}

fn instantiate_vesting_remote_chain(app: &mut App) -> Addr {
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let owner = Addr::unchecked(OWNER1);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = InstantiateMsg {
        owner: OWNER1.to_string(),
        vesting_token: native_asset_info(IBC_ASTRO.to_string()),
    };

    app.instantiate_contract(
        vesting_code_id,
        owner.clone(),
        &init_msg,
        &[],
        "Vesting",
        None,
    )
    .unwrap()
}

fn mint_tokens(app: &mut App, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER1), token.to_owned(), &msg, &[])
        .unwrap();
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn query_token_balance(app: &mut App, token: &Addr, address: &Addr) -> Uint128 {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: BalanceResponse = app.wrap().query_wasm_smart(token, &msg).unwrap();

    res.balance
}

fn claim_and_check(
    app: &mut App,
    who: &Addr,
    vesting: &Addr,
    astro_token: &Addr,
    expected_amount: u128,
) {
    app.execute_contract(
        who.clone(),
        vesting.clone(),
        &ExecuteMsg::Claim {
            recipient: None,
            amount: None,
        },
        &[],
    )
    .unwrap();
    let astro_amount = query_token_balance(app, &astro_token, &who);
    assert_eq!(astro_amount.u128(), expected_amount);
}
