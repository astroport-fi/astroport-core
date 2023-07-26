use crate::contract::{
    execute, execute_burn_from, execute_send_from, execute_transfer_from, instantiate,
    query_all_accounts, query_balance, query_balance_at,
};
use crate::state::get_total_supply_at;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    Addr, Binary, BlockInfo, ContractInfo, CosmosMsg, Deps, DepsMut, Env, StdError, SubMsg,
    Timestamp, Uint128, Uint64, WasmMsg,
};
use cw20::{
    AllAccountsResponse, BalanceResponse, Cw20Coin, Cw20ReceiveMsg, MinterResponse,
    TokenInfoResponse,
};
use cw20_base::allowances::execute_increase_allowance;
use cw20_base::contract::{query_minter, query_token_info};
use cw20_base::msg::{ExecuteMsg, InstantiateMsg};
use cw20_base::ContractError;

pub struct MockEnvParams {
    pub block_time: Timestamp,
    pub block_height: u64,
}

impl Default for MockEnvParams {
    fn default() -> Self {
        MockEnvParams {
            block_time: Timestamp::from_seconds(1_571_797_419),
            block_height: 1,
        }
    }
}

pub fn test_mock_env(mock_env_params: MockEnvParams) -> Env {
    Env {
        block: BlockInfo {
            height: mock_env_params.block_height,
            time: mock_env_params.block_time,
            chain_id: "cosmos-testnet-14002".to_string(),
        },
        transaction: None,
        contract: ContractInfo {
            address: Addr::unchecked(MOCK_CONTRACT_ADDR),
        },
    }
}

fn get_balance<T: Into<String>>(deps: Deps, address: T) -> Uint128 {
    query_balance(deps, address.into()).unwrap().balance
}

// This will set up the instantiation for other tests
fn do_instantiate_with_minter(
    deps: DepsMut,
    addr: &str,
    amount: Uint128,
    minter: &str,
    cap: Option<Uint128>,
) -> TokenInfoResponse {
    _do_instantiate(
        deps,
        addr,
        amount,
        Some(MinterResponse {
            minter: minter.to_string(),
            cap,
        }),
    )
}

// This will set up the instantiation for other tests without a minter
fn do_instantiate(deps: DepsMut, addr: &str, amount: Uint128) -> TokenInfoResponse {
    _do_instantiate(deps, addr, amount, None)
}

// This will set up the instantiation for other tests
fn _do_instantiate(
    mut deps: DepsMut,
    addr: &str,
    amount: Uint128,
    mint: Option<MinterResponse>,
) -> TokenInfoResponse {
    let instantiate_msg = InstantiateMsg {
        name: "Auto Gen".to_string(),
        symbol: "AUTO".to_string(),
        decimals: 3,
        initial_balances: vec![Cw20Coin {
            address: addr.to_string(),
            amount,
        }],
        mint: mint.clone(),
        marketing: None,
    };
    let info = mock_info("creator", &[]);
    let env = mock_env();
    let res = instantiate(deps.branch(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let meta = query_token_info(deps.as_ref()).unwrap();
    assert_eq!(
        meta,
        TokenInfoResponse {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            total_supply: amount,
        }
    );
    assert_eq!(get_balance(deps.as_ref(), addr), amount);
    assert_eq!(query_minter(deps.as_ref()).unwrap(), mint,);
    meta
}

mod instantiate {
    use super::*;

    #[test]
    fn basic() {
        let mut deps = mock_dependencies();
        let amount = Uint128::from(11223344u128);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20Coin {
                address: String::from("addr0000"),
                amount,
            }],
            mint: None,
            marketing: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                total_supply: amount,
            }
        );
        assert_eq!(
            get_balance(deps.as_ref(), "addr0000"),
            Uint128::new(11223344)
        );
    }

    #[test]
    fn mintable() {
        let mut deps = mock_dependencies();
        let amount = Uint128::new(11223344);
        let minter = String::from("asmodat");
        let limit = Uint128::new(511223344);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20Coin {
                address: "addr0000".into(),
                amount,
            }],
            mint: Some(MinterResponse {
                minter: minter.clone(),
                cap: Some(limit),
            }),
            marketing: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                total_supply: amount,
            }
        );
        assert_eq!(
            get_balance(deps.as_ref(), "addr0000"),
            Uint128::new(11223344)
        );
        assert_eq!(
            query_minter(deps.as_ref()).unwrap(),
            Some(MinterResponse {
                minter,
                cap: Some(limit),
            }),
        );
    }

    #[test]
    fn mintable_over_cap() {
        let mut deps = mock_dependencies();
        let amount = Uint128::new(11223344);
        let minter = String::from("asmodat");
        let limit = Uint128::new(11223300);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20Coin {
                address: String::from("addr0000"),
                amount,
            }],
            mint: Some(MinterResponse {
                minter,
                cap: Some(limit),
            }),
            marketing: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let err = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("Initial supply greater than cap").into()
        );
    }
}

#[test]
fn can_mint_by_minter() {
    let mut deps = mock_dependencies();

    let genesis = String::from("genesis");
    let amount = Uint128::new(11223344);
    let minter = String::from("asmodat");
    let limit = Uint128::new(511223344);
    do_instantiate_with_minter(deps.as_mut(), &genesis, amount, &minter, Some(limit));

    // Minter can mint coins to some winner
    let winner = String::from("lucky");
    let prize = Uint128::new(222_222_222);
    let msg = ExecuteMsg::Mint {
        recipient: winner.clone(),
        amount: prize,
    };

    let info = mock_info(minter.as_ref(), &[]);
    let env = mock_env();
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(0, res.messages.len());
    assert_eq!(get_balance(deps.as_ref(), genesis), amount);
    assert_eq!(get_balance(deps.as_ref(), winner.clone()), prize);

    // But cannot mint nothing
    let msg = ExecuteMsg::Mint {
        recipient: winner.clone(),
        amount: Uint128::zero(),
    };
    let info = mock_info(minter.as_ref(), &[]);
    let env = mock_env();
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidZeroAmount {});

    // But if it exceeds cap (even over multiple rounds), it fails
    let msg = ExecuteMsg::Mint {
        recipient: winner,
        amount: Uint128::new(333_222_222),
    };
    let info = mock_info(minter.as_ref(), &[]);
    let env = mock_env();
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::CannotExceedCap {});
}

#[test]
fn others_cannot_mint() {
    let mut deps = mock_dependencies();
    do_instantiate_with_minter(
        deps.as_mut(),
        &String::from("genesis"),
        Uint128::new(1234),
        &String::from("minter"),
        None,
    );

    let msg = ExecuteMsg::Mint {
        recipient: String::from("lucky"),
        amount: Uint128::new(222),
    };
    let info = mock_info("anyone else", &[]);
    let env = mock_env();
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::Unauthorized {});
}

#[test]
fn no_one_mints_if_minter_unset() {
    let mut deps = mock_dependencies();
    do_instantiate(deps.as_mut(), &String::from("genesis"), Uint128::new(1234));

    let msg = ExecuteMsg::Mint {
        recipient: String::from("lucky"),
        amount: Uint128::new(222),
    };
    let info = mock_info("genesis", &[]);
    let env = mock_env();
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::Unauthorized {});
}

#[test]
fn instantiate_multiple_accounts() {
    let mut deps = mock_dependencies();
    let amount1 = Uint128::from(11223344u128);
    let addr1 = String::from("addr0001");
    let amount2 = Uint128::from(7890987u128);
    let addr2 = String::from("addr0002");
    let instantiate_msg = InstantiateMsg {
        name: "Bash Shell".to_string(),
        symbol: "BASH".to_string(),
        decimals: 6,
        initial_balances: vec![
            Cw20Coin {
                address: addr1.clone(),
                amount: amount1,
            },
            Cw20Coin {
                address: addr2.clone(),
                amount: amount2,
            },
        ],
        mint: None,
        marketing: None,
    };
    let info = mock_info("creator", &[]);
    let env = mock_env();
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    assert_eq!(
        query_token_info(deps.as_ref()).unwrap(),
        TokenInfoResponse {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            total_supply: amount1 + amount2,
        }
    );
    assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
    assert_eq!(get_balance(deps.as_ref(), addr2), amount2);
}

#[test]
fn transfer() {
    let mut deps = mock_dependencies();
    let addr1 = String::from("addr0001");
    let addr2 = String::from("addr0002");
    let amount1 = Uint128::from(12340000u128);
    let transfer = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);

    do_instantiate(deps.as_mut(), &addr1, amount1);

    // Cannot transfer nothing
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Transfer {
        recipient: addr2.clone(),
        amount: Uint128::zero(),
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidZeroAmount {});

    // Cannot send more than we have
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Transfer {
        recipient: addr2.clone(),
        amount: too_much,
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

    // Cannot send from empty account
    let info = mock_info(addr2.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Transfer {
        recipient: addr1.clone(),
        amount: transfer,
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

    // Valid transfer
    let info = mock_info(addr1.as_ref(), &[]);
    let env = test_mock_env(MockEnvParams {
        block_height: 100_000,
        block_time: Timestamp::from_seconds(600_000),
    });
    let msg = ExecuteMsg::Transfer {
        recipient: addr2.clone(),
        amount: transfer,
    };
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let remainder = amount1.checked_sub(transfer).unwrap();
    assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
    assert_eq!(get_balance(deps.as_ref(), addr2.clone()), transfer);
    assert_eq!(
        query_balance_at(deps.as_ref(), addr1, Uint64::from(600_000u64))
            .unwrap()
            .balance,
        amount1
    );
    assert_eq!(
        query_balance_at(deps.as_ref(), addr2, Uint64::from(600_000u64))
            .unwrap()
            .balance,
        Uint128::zero()
    );
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        amount1
    );
}

#[test]
fn burn() {
    let mut deps = mock_dependencies();
    let addr1 = String::from("addr0001");
    let amount1 = Uint128::from(12340000u128);
    let burn = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);

    do_instantiate_with_minter(deps.as_mut(), &addr1, amount1, &addr1, None);

    // Cannot burn nothing
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Burn {
        amount: Uint128::zero(),
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidZeroAmount {});
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        amount1
    );

    // Cannot burn more than we have
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Burn { amount: too_much };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        amount1
    );

    // valid burn reduces total supply
    let info = mock_info(addr1.as_ref(), &[]);
    let env = test_mock_env(MockEnvParams {
        block_height: 200_000,
        block_time: Timestamp::from_seconds(1_200_000_000),
    });
    let msg = ExecuteMsg::Burn { amount: burn };
    execute(deps.as_mut(), env, info, msg).unwrap();

    let remainder = amount1.checked_sub(burn).unwrap();
    assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
    assert_eq!(
        query_balance_at(deps.as_ref(), addr1, Uint64::from(1_200_000_000u64))
            .unwrap()
            .balance,
        amount1
    );
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        remainder
    );
    assert_eq!(
        get_total_supply_at(&deps.storage, Uint64::from(1_200_000_000u64)).unwrap(),
        remainder
    );
}

#[test]
fn burn_unauthorized() {
    let mut deps = mock_dependencies();
    let addr1 = String::from("addr0001");
    let amount1 = Uint128::from(12340000u128);

    do_instantiate(deps.as_mut(), &addr1, amount1);

    // Cannot burn if we're not the minter
    let info = mock_info(addr1.as_ref(), &[]);
    let env = test_mock_env(MockEnvParams {
        block_height: 200_000,
        block_time: Timestamp::from_seconds(1_200_000_000),
    });
    let msg = ExecuteMsg::Burn { amount: amount1 };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::Unauthorized {});

    // Even though the call was unauthorised, ensure the balance is unchanged
    assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        amount1
    );
}

#[test]
fn send() {
    let mut deps = mock_dependencies();
    let addr1 = String::from("addr0001");
    let contract = String::from("addr0002");
    let amount1 = Uint128::from(12340000u128);
    let transfer = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);
    let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

    do_instantiate(deps.as_mut(), &addr1, amount1);

    // Cannot send nothing
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Send {
        contract: contract.clone(),
        amount: Uint128::zero(),
        msg: send_msg.clone(),
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidZeroAmount {});

    // Cannot send more than we have
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Send {
        contract: contract.clone(),
        amount: too_much,
        msg: send_msg.clone(),
    };
    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

    // Valid transfer
    let info = mock_info(addr1.as_ref(), &[]);
    let env = mock_env();
    let msg = ExecuteMsg::Send {
        contract: contract.clone(),
        amount: transfer,
        msg: send_msg.clone(),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Ensure proper send message was sent
    // This is the message we want delivered to the other side
    let binary_msg = Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: transfer,
        msg: send_msg,
    }
    .into_binary()
    .unwrap();
    // And this is how it must be wrapped for the vm to process it
    assert_eq!(
        res.messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract.clone(),
            msg: binary_msg,
            funds: vec![],
        }))
    );

    // Ensure balance is properly transferred
    let remainder = amount1.checked_sub(transfer).unwrap();
    assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
    assert_eq!(get_balance(deps.as_ref(), contract.clone()), transfer);
    assert_eq!(
        query_token_info(deps.as_ref()).unwrap().total_supply,
        amount1
    );
    assert_eq!(
        query_balance_at(deps.as_ref(), addr1, Uint64::from(env.block.time.seconds()))
            .unwrap()
            .balance,
        Uint128::zero()
    );
    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            contract,
            Uint64::from(env.block.time.seconds())
        )
        .unwrap()
        .balance,
        Uint128::zero()
    );
}

#[test]
fn snapshots_are_taken_and_retrieved_correctly() {
    let mut deps = mock_dependencies();

    let addr1 = String::from("addr1");
    let addr2 = String::from("addr2");

    let mut current_total_supply = Uint128::new(100_000);
    let mut current_block = 12_345;
    let mut current_time = Timestamp::from_seconds(1_571_797_419);
    let mut current_addr1_balance = current_total_supply;
    let mut current_addr2_balance = Uint128::zero();

    // Allow addr2 to burn tokens to check logic
    let minter = String::from("addr2");
    do_instantiate_with_minter(deps.as_mut(), &addr1, current_total_supply, &minter, None);

    let mut expected_total_supplies = vec![(current_time.seconds(), current_total_supply)];
    let mut expected_addr1_balances = vec![(current_time.seconds(), current_addr1_balance)];
    let mut expected_addr2_balances: Vec<(u64, Uint128)> = vec![];

    // Mint to addr2 3 times
    for _i in 0..3 {
        current_block += 100_000;
        current_time = current_time.plus_seconds(600_000);

        let mint_amount = Uint128::new(20_000);
        current_total_supply += mint_amount;
        current_addr2_balance += mint_amount;

        let info = mock_info(minter.as_str(), &[]);
        let env = test_mock_env(MockEnvParams {
            block_height: current_block,
            block_time: current_time,
        });

        let msg = ExecuteMsg::Mint {
            recipient: addr2.clone(),
            amount: mint_amount,
        };

        execute(deps.as_mut(), env, info, msg).unwrap();

        expected_total_supplies.push((current_time.seconds(), current_total_supply));
        expected_addr2_balances.push((current_time.seconds(), current_addr2_balance));
    }

    // Transfer from addr1 to addr2 4 times
    for _i in 0..4 {
        current_block += 60_000;
        current_time = current_time.plus_seconds(360_000);

        let transfer_amount = Uint128::new(10_000);
        current_addr1_balance -= transfer_amount;
        current_addr2_balance += transfer_amount;

        let info = mock_info(addr1.as_str(), &[]);
        let env = test_mock_env(MockEnvParams {
            block_height: current_block,
            block_time: current_time,
        });

        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer_amount,
        };

        execute(deps.as_mut(), env, info, msg).unwrap();

        expected_addr1_balances.push((current_time.seconds(), current_addr1_balance));
        expected_addr2_balances.push((current_time.seconds(), current_addr2_balance));
    }

    // Burn from addr2 3 times
    for _i in 0..3 {
        current_block += 50_000;
        current_time = current_time.plus_seconds(300_000);

        let burn_amount = Uint128::new(20_000);
        current_total_supply -= burn_amount;
        current_addr2_balance -= burn_amount;

        let info = mock_info(addr2.as_str(), &[]);

        let env = test_mock_env(MockEnvParams {
            block_height: current_block,
            block_time: current_time,
        });

        let msg = ExecuteMsg::Burn {
            amount: burn_amount,
        };

        execute(deps.as_mut(), env, info, msg).unwrap();

        expected_total_supplies.push((current_time.seconds(), current_total_supply));
        expected_addr2_balances.push((current_time.seconds(), current_addr2_balance));
    }

    // Check total supply
    let mut total_supply_previous_value = Uint128::zero();
    for (timestamp, expected_total_supply) in expected_total_supplies {
        // Previous second gives previous value
        assert_eq!(
            get_total_supply_at(&deps.storage, Uint64::from(timestamp - 1)).unwrap(),
            total_supply_previous_value
        );

        // Current second gives expected value
        assert_eq!(
            get_total_supply_at(&deps.storage, Uint64::from(timestamp)).unwrap(),
            expected_total_supply,
        );

        // Next second still gives expected value
        assert_eq!(
            get_total_supply_at(&deps.storage, Uint64::from(timestamp + 10)).unwrap(),
            expected_total_supply,
        );

        total_supply_previous_value = expected_total_supply;
    }

    // Check addr1 balances
    let mut balance_previous_value = Uint128::zero();
    for (timestamp, expected_balance) in expected_addr1_balances {
        // Previous second gives previous value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1.clone(), Uint64::from(timestamp - 10))
                .unwrap()
                .balance,
            balance_previous_value
        );

        // Current second gives previous value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1.clone(), Uint64::from(timestamp))
                .unwrap()
                .balance,
            balance_previous_value
        );

        // Only the next second still gives expected value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1.clone(), Uint64::from(timestamp + 1))
                .unwrap()
                .balance,
            expected_balance
        );

        balance_previous_value = expected_balance;
    }

    // Check addr2 balances
    let mut balance_previous_value = Uint128::zero();
    for (timestamp, expected_balance) in expected_addr2_balances {
        // Previous second gives previous value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr2.clone(), Uint64::from(timestamp - 10))
                .unwrap()
                .balance,
            balance_previous_value
        );

        // The current second gives the previous value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr2.clone(), Uint64::from(timestamp))
                .unwrap()
                .balance,
            balance_previous_value
        );

        // Only the next second still gives expected value
        assert_eq!(
            query_balance_at(deps.as_ref(), addr2.clone(), Uint64::from(timestamp + 1))
                .unwrap()
                .balance,
            expected_balance
        );

        balance_previous_value = expected_balance;
    }
}

#[test]
fn test_balance_history() {
    let mut deps = mock_dependencies();
    let user1 = mock_info("user1", &[]);
    do_instantiate_with_minter(
        deps.as_mut(),
        user1.sender.as_str(),
        Uint128::new(1_000),
        "user2",
        None,
    );

    // Test transfer_from
    let mut env = mock_env();
    env.block.height += 1;
    env.block.time = env.block.time.plus_seconds(1);
    let user2 = mock_info("user2", &[]);

    execute_increase_allowance(
        deps.as_mut(),
        env.clone(),
        user1.clone(),
        user2.sender.to_string(),
        Uint128::new(1000),
        None,
    )
    .unwrap();

    execute_transfer_from(
        deps.as_mut(),
        env.clone(),
        user2.clone(),
        user1.sender.to_string(),
        user2.sender.to_string(),
        Uint128::new(1),
    )
    .unwrap();

    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user1.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(1000)
        }
    );
    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user2.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(0)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user1.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(999)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user2.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );

    // Test burn_from
    let mut env = mock_env();
    env.block.height += 2;
    env.block.time = env.block.time.plus_seconds(2);

    execute_burn_from(
        deps.as_mut(),
        env.clone(),
        user2.clone(),
        user1.sender.to_string(),
        Uint128::new(1),
    )
    .unwrap();

    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user1.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(999)
        }
    );
    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user2.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user1.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(998)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user2.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );

    // Test send_from
    let mut env = mock_env();
    env.block.height += 3;
    env.block.time = env.block.time.plus_seconds(3);

    execute_send_from(
        deps.as_mut(),
        env.clone(),
        user2.clone(),
        user1.sender.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::new(1),
        Binary::default(),
    )
    .unwrap();

    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user1.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(998)
        }
    );
    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            user2.sender.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );
    assert_eq!(
        query_balance_at(
            deps.as_ref(),
            MOCK_CONTRACT_ADDR.to_string(),
            Uint64::from(env.block.time.seconds())
        )
        .unwrap(),
        BalanceResponse {
            balance: Uint128::new(0)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user1.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(997)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), user2.sender.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );
    assert_eq!(
        query_balance(deps.as_ref(), MOCK_CONTRACT_ADDR.to_string()).unwrap(),
        BalanceResponse {
            balance: Uint128::new(1)
        }
    );

    // Test query_all_accounts
    assert_eq!(
        query_all_accounts(deps.as_ref(), None, None).unwrap(),
        AllAccountsResponse {
            accounts: vec![
                MOCK_CONTRACT_ADDR.to_string(),
                user1.sender.to_string(),
                user2.sender.to_string(),
            ]
        }
    );
}
