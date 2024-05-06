#![cfg(not(tarpaulin_include))]

use std::collections::HashMap;

use cosmwasm_std::{coin, coins, from_json, Addr, BlockInfo, Timestamp, Uint128};
use cw_multi_test::{Executor, TOKEN_FACTORY_MODULE};
use cw_utils::PaymentError;
use itertools::Itertools;

use astroport::staking::{Config, ExecuteMsg, QueryMsg, StakingResponse, TrackerData};
use astroport_staking::error::ContractError;

use crate::common::helper::{Helper, ASTRO_DENOM};

mod common;

#[test]
fn test_instantiate() {
    let owner = Addr::unchecked("owner");

    let helper = Helper::new(&owner).unwrap();

    let response: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        response,
        Config {
            astro_denom: ASTRO_DENOM.to_string(),
            xastro_denom: format!("factory/{}/xASTRO", &helper.staking)
        }
    );

    let response: TrackerData = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::TrackerConfig {})
        .unwrap();
    assert_eq!(
        response,
        TrackerData {
            code_id: 2,
            admin: owner.to_string(),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
            tracker_addr: "contract1".to_string(),
        }
    );
}

#[test]
fn check_deflate_liquidity() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();

    let attacker = Addr::unchecked("attacker");
    let victim = Addr::unchecked("victim");

    helper.give_astro(10000, &attacker);
    helper.give_astro(10000, &victim);

    let err = helper.stake(&attacker, 1000).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::MinimumStakeAmountError {}
    );

    helper.stake(&attacker, 1001).unwrap();

    helper
        .app
        .send_tokens(
            attacker.clone(),
            helper.staking.clone(),
            &coins(5000, ASTRO_DENOM),
        )
        .unwrap();

    let err = helper.stake(&victim, 5).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::StakeAmountTooSmall {}
    );

    helper.stake(&victim, 7).unwrap();
}

#[test]
fn test_invalid_denom() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();

    let bad_denom = "bad/denom";
    helper.mint_coin(&owner, coin(1000, bad_denom));

    // Try to stake bad denom
    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.staking.clone(),
            &ExecuteMsg::Enter { receiver: None },
            &coins(1000u128, bad_denom),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MissingDenom(ASTRO_DENOM.to_string()))
    );

    // Try to stake bad denom along with ASTRO
    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.staking.clone(),
            &ExecuteMsg::Enter { receiver: None },
            &[coin(1000u128, bad_denom), coin(1000u128, ASTRO_DENOM)],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MultipleDenoms {})
    );

    // Stake to pass xASTRO bank module balance check below
    helper.stake(&owner, 10000).unwrap();

    // Try to unstake bad denom
    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.staking.clone(),
            &ExecuteMsg::Leave {},
            &coins(1000u128, bad_denom),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MissingDenom(helper.xastro_denom.to_string()))
    );

    // Try to unstake bad denom along with xASTRO
    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            helper.staking.clone(),
            &ExecuteMsg::Leave {},
            &[
                coin(1000u128, bad_denom),
                coin(1000u128, helper.xastro_denom.clone()),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MultipleDenoms {})
    );
}

#[test]
fn test_enter_and_leave() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();
    let xastro_denom = helper.xastro_denom.clone();
    let staking = helper.staking.clone();

    let alice = Addr::unchecked("alice");

    // Mint 10000 ASTRO for Alice
    helper.give_astro(10000, &alice);

    // Stake Alice's 1100 ASTRO for 1100 xASTRO
    let resp_data = helper.stake(&alice, 1100).unwrap().data.unwrap();
    let staking_resp: StakingResponse = from_json(&resp_data).unwrap();
    assert_eq!(
        staking_resp,
        StakingResponse {
            astro_amount: 1100u128.into(),
            xastro_amount: 100u128.into(),
        }
    );

    // Check if Alice's xASTRO balance is 100 (1000 consumed by staking contract on initial provide)
    let amount = helper.query_balance(&alice, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 100);

    // Check if the staking contract's ASTRO balance is 1100
    let amount = helper.query_balance(&staking, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 1100u128);

    // Unstake Alice's 10 xASTRO for 10 ASTRO
    let resp_data = helper.unstake(&alice, 10).unwrap().data.unwrap();
    let staking_resp: StakingResponse = from_json(&resp_data).unwrap();
    assert_eq!(
        staking_resp,
        StakingResponse {
            astro_amount: 10u128.into(),
            xastro_amount: 10u128.into(),
        }
    );

    // Check if Alice's xASTRO balance is 90
    let amount = helper.query_balance(&alice, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 90);

    // Check if Alice's ASTRO balance is 8910
    let amount = helper.query_balance(&alice, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 8910);

    // Check if the staking contract's ASTRO balance is 1090
    let amount = helper.query_balance(&staking, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 1090);

    // Check if the staking contract's xASTRO balance is 1000 (locked forever)
    let amount = helper.query_balance(&staking, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 1000);

    // Check staking for specific recipient
    let user = Addr::unchecked("user");
    let recipient = Addr::unchecked("recipient");
    helper.give_astro(10000, &user);
    helper
        .app
        .execute_contract(
            user.clone(),
            helper.staking.clone(),
            &ExecuteMsg::Enter {
                receiver: Some(recipient.to_string()),
            },
            &coins(10000, ASTRO_DENOM),
        )
        .unwrap();

    let amount = helper.query_balance(&user, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 0);

    let amount = helper.query_balance(&recipient, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 10000);
}

#[test]
fn should_work_with_more_than_one_participant() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();
    let xastro_denom = helper.xastro_denom.clone();
    let staking = helper.staking.clone();

    let alice = Addr::unchecked("alice");
    let bob = Addr::unchecked("bob");

    // Mint 10000 ASTRO for Alice and Bob
    helper.give_astro(10000, &alice);
    helper.give_astro(10000, &bob);

    // Stake Alice's 2000 ASTRO for 1000 xASTRO (subtract min liquid amount)
    helper.stake(&alice, 2000).unwrap();
    // Check Alice's xASTRO balance is 1000
    let amount = helper.query_balance(&alice, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 1000);

    // Stake Bob's 10 ASTRO for 10 xASTRO
    helper.stake(&bob, 10).unwrap();
    // Check Bob's xASTRO balance is 10
    let amount = helper.query_balance(&bob, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 10);

    // Check staking contract's ASTRO balance is 2010
    let amount = helper.query_balance(&staking, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 2010);

    // Staking contract gets 20 more ASTRO from external source
    helper.give_astro(20, &staking);

    // Stake Alice's 10 ASTRO for 9 xASTRO: 10*2010/2030 = 9
    helper.stake(&alice, 10).unwrap();

    // Check Alice's xASTRO balance is 1009
    let amount = helper.query_balance(&alice, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 1009);

    // Burn Bob's 5 xASTRO and unstake: gets 5*2040/2019 = 5 ASTRO
    helper.unstake(&bob, 5).unwrap();
    // Check Bob's xASTRO balance is 5
    let amount = helper.query_balance(&bob, &xastro_denom).unwrap();
    assert_eq!(amount.u128(), 5);
    // Check Bob's ASTRO balance is 9995 (10000 minted - 10 entered + 5 by leaving)
    let amount = helper.query_balance(&bob, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 9995);

    // Check the staking contract's ASTRO balance
    let amount = helper.query_balance(&staking, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 2035);

    // Check Alice's ASTRO balance is 7990 (10000 minted - 2000 entered - 10 entered)
    let amount = helper.query_balance(&alice, ASTRO_DENOM).unwrap();
    assert_eq!(amount.u128(), 7990);
}

#[test]
fn test_historical_queries() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();
    helper.app.set_block(BlockInfo {
        height: 1000,
        time: Timestamp::from_seconds(1700000000),
        chain_id: "".to_string(),
    });

    helper.stake(&owner, 1001).unwrap();

    let xastro_denom = helper.xastro_denom.clone();

    let user1 = Addr::unchecked("user1");
    let user2 = Addr::unchecked("user2");

    // Stake and query at the same block
    helper.give_astro(1_000_000000, &user1);
    helper.stake(&user1, 1_000_000000).unwrap();

    let amount = helper.query_xastro_balance_at(&user1, None).unwrap();
    assert_eq!(amount.u128(), 1_000_000000);
    let total_supply = helper.query_xastro_supply_at(None).unwrap();
    assert_eq!(total_supply.u128(), 1_000_001001);

    // Stake for user2 too
    helper.give_astro(1_000_000000, &user2);
    helper.stake(&user2, 1_000_000000).unwrap();

    struct Entry {
        user1_bal: Uint128,
        user2_bal: Uint128,
        total_supply: Uint128,
    }
    let mut history: HashMap<u64, Entry> = Default::default();

    for _ in 0..10 {
        helper.next_block(100);

        helper
            .app
            .send_tokens(
                user1.clone(),
                user2.clone(),
                &coins(1_000000, &xastro_denom),
            )
            .unwrap();

        // Stake to impact total supply
        helper.give_astro(2_000000, &user1);
        helper.stake(&user1, 2_000000).unwrap();

        // Unstake to impact total supply
        helper.unstake(&user2, 3_000000).unwrap();

        history.insert(
            helper.app.block_info().time.seconds() + 1, // balance change takes effect from the next block
            Entry {
                user1_bal: helper
                    .app
                    .wrap()
                    .query_balance(&user1, &xastro_denom)
                    .unwrap()
                    .amount,
                user2_bal: helper
                    .app
                    .wrap()
                    .query_balance(&user2, &xastro_denom)
                    .unwrap()
                    .amount,
                total_supply: helper
                    .app
                    .wrap()
                    .query_supply(&xastro_denom)
                    .unwrap()
                    .amount,
            },
        );
    }

    for (
        timestamp,
        Entry {
            user1_bal,
            user2_bal,
            total_supply,
        },
    ) in history.into_iter().sorted_by(|(t1, _), (t2, _)| t1.cmp(t2))
    {
        let historical_user1_bal = helper
            .query_xastro_balance_at(&user1, Some(timestamp))
            .unwrap();
        assert_eq!(
            historical_user1_bal, user1_bal,
            "Invalid balance for user1 at {timestamp}"
        );

        let historical_user2_bal = helper
            .query_xastro_balance_at(&user2, Some(timestamp))
            .unwrap();
        assert_eq!(
            historical_user2_bal, user2_bal,
            "Invalid balance for user2 at {timestamp}"
        );

        let historical_total_supply = helper.query_xastro_supply_at(Some(timestamp)).unwrap();
        assert_eq!(
            historical_total_supply, total_supply,
            "Invalid total supply at {timestamp}"
        );
    }

    // Check the rest of the queries

    let total_shares: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::TotalShares {})
        .unwrap();
    let total_supply = helper
        .app
        .wrap()
        .query_supply(&xastro_denom)
        .unwrap()
        .amount;
    assert_eq!(total_shares, total_supply);

    let staking = helper.staking.clone();
    let total_deposit: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.staking, &QueryMsg::TotalDeposit {})
        .unwrap();
    let staking_astro_balance = helper
        .app
        .wrap()
        .query_balance(&staking, ASTRO_DENOM)
        .unwrap()
        .amount;
    assert_eq!(total_deposit, staking_astro_balance);
}

#[test]
fn test_different_query_results() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();
    let alice = Addr::unchecked("alice");
    // Mint 10000 ASTRO for Alice
    helper.give_astro(10000, &alice);
    // Stake Alice's 1100 ASTRO for 1100 xASTRO
    let resp_data = helper.stake(&alice, 1100).unwrap().data.unwrap();
    let staking_resp: StakingResponse = from_json(&resp_data).unwrap();
    assert_eq!(
        staking_resp,
        StakingResponse {
            astro_amount: 1100u128.into(),
            xastro_amount: 100u128.into(),
        }
    );
    // get current time
    let time_now = helper.app.block_info().time.seconds();
    // query with None, which uses deps.querier.query_balance
    let total_supply_none: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.staking,
            &QueryMsg::TotalSupplyAt { timestamp: None },
        )
        .unwrap();
    // query with Some(_), which uses SnapshotMap
    let total_supply_some: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.staking,
            &QueryMsg::TotalSupplyAt {
                timestamp: Some(time_now),
            },
        )
        .unwrap();
    assert_eq!(total_supply_none, total_supply_some);

    let balance_none: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.staking,
            &QueryMsg::BalanceAt {
                timestamp: None,
                address: alice.to_string(),
            },
        )
        .unwrap();
    let balance_some: Uint128 = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.staking,
            &QueryMsg::BalanceAt {
                timestamp: Some(time_now),
                address: alice.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance_none, balance_some);
}
