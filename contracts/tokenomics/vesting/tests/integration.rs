use astroport::vesting::{QueryMsg, VestingAccountResponse};
use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg, ExecuteMsg, InstantiateMsg, VestingAccount, VestingSchedule,
        VestingSchedulePoint,
    },
};
use astroport_vesting::state::Config;
use cosmwasm_std::Coin;
use cosmwasm_std::{
    to_binary, Addr, Timestamp, Uint128,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account};

const TOKEN_INITIAL_AMOUNT: u128 = 1_000_000_000_000000;

#[test]
fn claim() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        2
    ).unwrap();

    let owner = &accs[0];
    let user1 = &accs[1];

    let token_code_id = store_token_code(&wasm, owner);

    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let vesting_instance = instantiate_vesting(&wasm, owner, &astro_token_instance);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.address(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101).seconds(),
                            amount: Uint128::new(200),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    let res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(res.to_string(), "Vesting schedule amount error. Schedules total amount should be equal to cw20 receive amount.");

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.address(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.address(),
    };

    let user1_vesting_amount: Uint128 = wasm.query(vesting_instance.as_str(), &msg).unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(300u128));

    // check owner balance
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    // check vesting balance
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        300u128,
    );

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = wasm.execute(vesting_instance.as_str(), &msg, &[], user1).unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.address(),
    };

    let vesting_res: VestingAccountResponse = wasm.query(vesting_instance.as_str(), &msg).unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(300u128));

    // check vesting balance
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        0u128,
    );

    //check user balance
    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 300u128);

    // owner balance mustn't change after claim
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    let msg = QueryMsg::AvailableAmount {
        address: user1.address(),
    };

    // check user balance after claim
    let user1_vesting_amount: Uint128 = wasm.query(vesting_instance.as_str(), &msg).unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(0u128));
}

#[test]
fn register_vesting_accounts() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();

    let owner = &accs[0];
    let user1 = &accs[1];
    let user2 = &accs[2];

    let token_code_id = store_token_code(&wasm, owner);

    let astro_token_instance = instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let noname_token_instance = instantiate_token(
        &wasm,
        owner,
        token_code_id,
        "NONAME",
        Some(1_000_000_000_000000),
    );

    mint_tokens(
        &wasm,
        owner,
        &noname_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT,
    );

    let vesting_instance = instantiate_vesting(&wasm, owner, &astro_token_instance);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.address(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(res.to_string(), "Vesting schedule error on addr: user1. Should satisfy: (start < end and at_start < total) or (start = end and at_start = total)");

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.address(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = wasm.execute(astro_token_instance.as_str(), &msg, &[], user1).unwrap_err();
    assert_eq!(res.to_string(), "Overflow: Cannot Sub with 0 and 100");

    let res = wasm.execute(noname_token_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(res.to_string(), "Unauthorized");

    let _res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();
    let msg = QueryMsg::AvailableAmount {
        address: user1.address(),
    };

    let user1_vesting_amount: Uint128 = wasm.query(vesting_instance.as_str(), &msg).unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(100u128));
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 100u128,
    );
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        100u128,
    );

    // let's check user1 final vesting amount after add schedule for a new one
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user2.address(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::new(200),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(200u128),
    };

    let _res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();
    let msg = QueryMsg::AvailableAmount {
        address: user2.address(),
    };

    let user2_vesting_amount: Uint128 = wasm.query(vesting_instance.as_str(), &msg).unwrap();

    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        300u128,
    );
    // A new schedule have been added successfully and an old one haven't changed.
    // The new one doesn't have the same value as old one.
    assert_eq!(user2_vesting_amount, Uint128::new(200u128));
    assert_eq!(user1_vesting_amount, Uint128::from(100u128));

    // add one more vesting schedule account, final vesting amount must increase only
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.address(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(200).seconds(),
                        amount: Uint128::new(10),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(10u128),
    };

    let _res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();
    let msg = QueryMsg::AvailableAmount {
        address: user1.address(),
    };

    let vesting_res: Uint128 = wasm.query(vesting_instance.as_str(), &msg).unwrap();
    assert_eq!(vesting_res, Uint128::new(110u128));
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        310u128,
    );

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = wasm.execute(vesting_instance.as_str(), &msg, &[], user1).unwrap();
    let msg = QueryMsg::VestingAccount {
        address: user1.address(),
    };

    let vesting_res: VestingAccountResponse = wasm.query(vesting_instance.as_str(), &msg).unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(110u128));
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &vesting_instance.clone(),
        200u128,
    );
    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 110u128);
    // owner balance mustn't change after claim
    check_token_balance(
        &wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
}

fn store_token_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let astro_token_contract = std::fs::read("../../../../artifacts/astroport_token.wasm").unwrap();
    let contract = wasm.store_code(&astro_token_contract, None, owner).unwrap();
    contract.data.code_id
}

fn instantiate_token(
    wasm: &Wasm<TerraTestApp>,
    owner: &SigningAccount,
    token_code_id: u64,
    name: &str,
    cap: Option<u128>,
) -> Addr {
    let name = String::from(name);

    let msg = TokenInstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.address(),
            cap: cap.map(|v| Uint128::from(v)),
        }),
        marketing: None,
    };

    Addr::unchecked(wasm.instantiate(
        token_code_id, 
        &msg, 
        Some(&owner.address()), 
        Some(&name), 
        &[], 
        owner
    ).unwrap().data.address)
}

fn instantiate_vesting(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount, astro_token_instance: &Addr) -> Addr {
    let vesting_contract = std::fs::read("../../../../artifacts/astroport_staking.wasm").unwrap();
    let vesting_code_id = wasm.store_code(&vesting_contract, None, owner).unwrap().data.code_id;

    let init_msg = InstantiateMsg {
        owner: owner.address(),
        token_addr: astro_token_instance.to_string(),
    };

    let vesting_instance = wasm.instantiate(
        vesting_code_id, 
        &init_msg, 
        Some(&owner.address()), 
        Some("Vesting"), 
        &[], 
        owner
    ).unwrap();

    let res: Config = wasm
        .query(&vesting_instance.data.address, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(astro_token_instance.to_string(), res.token_addr.to_string());

    mint_tokens(
        wasm,
        owner,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT,
    );

    check_token_balance(
        wasm,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        TOKEN_INITIAL_AMOUNT,
    );

    Addr::unchecked(vesting_instance.data.address)
}

fn mint_tokens(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    wasm.execute(
        token.as_str(), 
        &msg, 
        &[], 
        owner
    ).unwrap();
}

fn check_token_balance(wasm: &Wasm<TerraTestApp>, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: BalanceResponse = wasm.query(token.as_str(), &msg).unwrap();
    assert_eq!(res.balance, Uint128::from(expected));
}