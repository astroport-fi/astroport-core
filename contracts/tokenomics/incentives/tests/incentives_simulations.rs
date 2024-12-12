#![allow(dead_code)]
extern crate core;

use std::collections::{HashMap, HashSet};

use cosmwasm_std::{StdError, Timestamp};
use itertools::Itertools;
use proptest::prelude::*;

use astroport::asset::{AssetInfo, AssetInfoExt};
use astroport::incentives::{MAX_PERIODS, MAX_REWARD_TOKENS};
use astroport_incentives::error::ContractError;
use Event::*;

use crate::helper::{Helper, TestAddr};

mod helper;
const MAX_EVENTS: usize = 100;
const MAX_POOLS: u8 = 3;
const MAX_USERS: u8 = 3;

#[derive(Debug)]
enum Event {
    Deposit {
        sender_id: u8,
        lp_token_id: u8,
        amount: u8,
    },
    Withdraw {
        sender_id: u8,
        lp_token_id: u8,
        amount: u8,
    },
    Claim {
        sender_id: u8,
    },
    Incentivize {
        lp_token_id: u8,
        reward_id: u8,
        amount: u128,
        duration: u64,
    },
    SetupPools {
        pools: Vec<(u8, u8)>,
        tokens_per_second: u128,
    },
    RemoveReward {
        lp_token_id: u8,
        reward_id: u8,
    },
}

fn get_transfer_amount(events: &[cosmwasm_std::Event], denom: &str) -> u128 {
    events
        .iter()
        .find(|event| event.ty == "transfer")
        .and_then(|event| {
            event
                .attributes
                .iter()
                .find(|attr| attr.key == "amount" && attr.value.ends_with(denom))
                .map(|attr| {
                    attr.value
                        .strip_suffix(denom)
                        .unwrap()
                        .parse::<u128>()
                        .unwrap()
                })
        })
        .unwrap_or(0)
}

#[derive(Default, Debug)]
struct RewardData {
    pub left: u128,
    pub total: u128,
}

type RewardsAccounting = HashMap<String, HashMap<String, RewardData>>;

fn update_total_rewards(
    events: &[cosmwasm_std::Event],
    lp_token: &str,
    rewards_left: &mut RewardsAccounting,
) {
    if let Some(lp_map) = rewards_left.get_mut(lp_token) {
        let rewards = lp_map.keys().cloned().collect_vec();
        for reward in rewards {
            let claimed_amount = get_transfer_amount(events, &reward);
            if claimed_amount > 0 {
                println!("Claimed {claimed_amount} {reward} for {lp_token}");
                lp_map
                    .entry(reward.clone())
                    .and_modify(|v| {
                        v.left = v.left.checked_sub(claimed_amount).expect(&format!(
                            "Tried to claim more than available: {v:?} - {claimed_amount}"
                        ))
                    })
                    .or_insert_with(|| {
                        panic!("Reward {reward} not found in {lp_token} rewards map");
                    });
            }
        }
    }
}

fn simulate_case(events: Vec<(Event, u64)>) {
    let astro = AssetInfo::native("astro");
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    // How many tokens we need to produce MAX_POOLS unique pairs?
    // solve n(n-1) / 2 = MAX_POOLS against n, select only positive result and round it up
    let tokens_number = ((1.0 + (1.0 + 8.0 * MAX_POOLS as f64).sqrt()) / 2.0).ceil() as usize;

    let users = (0..MAX_USERS)
        .into_iter()
        .map(|i| TestAddr::new(&format!("user{i}")))
        .collect_vec();
    let mut user_positions = HashMap::new();

    // Create MAX_POOLS pairs and provide liquidity for them for each user
    let lp_tokens = (1..=tokens_number)
        .cartesian_product(1..=tokens_number)
        .filter(|(a, b)| a != b)
        .filter_map(|(a, b)| {
            let asset_infos = [
                AssetInfo::native(format!("token{a}")),
                AssetInfo::native(format!("token{b}")),
            ];
            let pair_info = helper.create_pair(&asset_infos).ok()?;

            let provide_assets = [
                asset_infos[0].with_balance(100_000_00000u64),
                asset_infos[1].with_balance(100_000_00000u64),
            ];
            // Owner provides liquidity first just to make following calculations easier
            // since first depositor gets small cut of LP tokens
            helper
                .provide_liquidity(
                    &owner,
                    &provide_assets,
                    &pair_info.contract_addr,
                    false, // Owner doesn't stake in incentives
                )
                .unwrap();

            for user in &users {
                helper
                    .provide_liquidity(
                        &user,
                        &provide_assets,
                        &pair_info.contract_addr,
                        false, // Do not stake on test initialization
                    )
                    .unwrap();
            }

            Some(pair_info.liquidity_token.to_string())
        })
        .take(MAX_POOLS as usize)
        .collect_vec();

    let bank = TestAddr::new("bank");
    let dereg_rewards_receiver = TestAddr::new("dereg_rewards_receiver");

    let mut rewards: RewardsAccounting = HashMap::new();
    let mut longest_schedule_end = helper.app.block_info().time.seconds();

    for (i, (event, shift_time)) in events.into_iter().enumerate() {
        println!(
            "Event {i} at {}: {event:?}",
            helper.app.block_info().time.seconds()
        );
        match event {
            Deposit {
                sender_id,
                lp_token_id,
                amount,
            } => {
                let user = &users[sender_id as usize];
                let lp_token = &lp_tokens[lp_token_id as usize];
                let lp_asset_info = AssetInfo::native(lp_token);
                let total_amount = lp_asset_info.query_pool(&helper.app.wrap(), user).unwrap();
                let part = total_amount.u128() * amount as u128 / 100;

                let resp = helper
                    .stake(user, lp_asset_info.with_balance(part))
                    .unwrap();

                update_total_rewards(&resp.events, lp_token, &mut rewards);

                user_positions
                    .entry(user)
                    .or_insert(HashSet::new())
                    .insert(lp_token.clone());
            }
            Withdraw {
                sender_id,
                lp_token_id,
                amount,
            } => {
                let user = &users[sender_id as usize];
                let lp_token = &lp_tokens[lp_token_id as usize];
                if let Ok(total_amount) = helper.query_deposit(lp_token, user) {
                    let part = total_amount * amount as u128 / 100;
                    let resp = helper.unstake(user, lp_token, part).unwrap();

                    update_total_rewards(&resp.events, lp_token, &mut rewards);

                    if amount == 100 {
                        user_positions
                            .entry(user)
                            .or_insert(HashSet::new())
                            .remove(lp_token);
                    }
                }
            }
            Claim { sender_id } => {
                let user = &users[sender_id as usize];
                for lp_token in user_positions.get(user).unwrap_or(&HashSet::new()) {
                    println!("{user} claims rewards for {lp_token}");
                    let resp = helper.claim_rewards(user, vec![lp_token.clone()]).unwrap();

                    update_total_rewards(&resp.events, lp_token, &mut rewards);
                }
            }
            Incentivize {
                lp_token_id,
                reward_id,
                amount,
                duration,
            } => {
                let lp_token = &lp_tokens[lp_token_id as usize];
                let reward_token = AssetInfo::native(format!("reward{reward_id}"));
                // ignore invalid schedules
                if let Ok((schedule, int_sch)) =
                    helper.create_schedule(&reward_token.with_balance(amount), duration)
                {
                    longest_schedule_end = longest_schedule_end.max(int_sch.end_ts);

                    let mut attach_funds = vec![];
                    if helper.is_fee_needed(lp_token, &reward_token) {
                        helper.mint_coin(&bank, &incentivization_fee);
                        attach_funds.push(incentivization_fee.clone());
                    }
                    helper.mint_assets(&bank, &[schedule.reward.clone()]);
                    helper
                        .incentivize(&bank, &lp_token, schedule, &attach_funds)
                        .unwrap();

                    let r = rewards
                        .entry(lp_token.to_string())
                        .or_insert_with(HashMap::new)
                        .entry(reward_token.to_string())
                        .or_insert(RewardData::default());
                    r.left += amount;
                    r.total += amount;
                }
            }
            SetupPools {
                pools,
                tokens_per_second,
            } => {
                let pools = pools
                    .into_iter()
                    .map(|(lp_token_id, alloc_points)| {
                        let lp_token = &lp_tokens[lp_token_id as usize];
                        (lp_token.clone(), alloc_points as u128)
                    })
                    .sorted_by(|a, b| a.0.cmp(&b.0))
                    .dedup_by(|a, b| a.0 == b.0)
                    .collect_vec();

                helper.setup_pools(pools).unwrap();
                helper.set_tokens_per_second(tokens_per_second).unwrap();
            }
            RemoveReward {
                lp_token_id,
                reward_id,
            } => {
                let lp_token = &lp_tokens[lp_token_id as usize];
                let reward = format!("reward{reward_id}");
                let res =
                    helper.remove_reward(&owner, lp_token, &reward, false, &dereg_rewards_receiver);

                match res {
                    Ok(resp) => {
                        let removed_amount = get_transfer_amount(&resp.events, &reward);
                        rewards
                            .get_mut(lp_token)
                            .unwrap()
                            .entry(reward)
                            .and_modify(|v| {
                                v.left = v.left.checked_sub(removed_amount).expect(&format!(
                                    "Tried to remove more than available: {v:?} - {removed_amount}"
                                ))
                            });
                    }
                    Err(err) => {
                        let err = err.downcast::<ContractError>().unwrap();
                        match err {
                            ContractError::Std(StdError::NotFound { .. })
                            | ContractError::RewardNotFound { .. } => {
                                // ignore
                            }
                            unexpected_err => panic!("Unexpected error: {unexpected_err:?}"),
                        }
                    }
                }
            }
        }

        helper.next_block(shift_time)
    }

    // Collect all rewards till the end of the longest schedule

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(longest_schedule_end + 1);
        block.height += 1
    });

    for (user, lp_tokens) in user_positions {
        for lp_token in lp_tokens {
            let resp = helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
            update_total_rewards(&resp.events, &lp_token, &mut rewards);
        }
    }

    // Collect orphaned rewards.
    match helper.claim_orphaned_rewards(None, dereg_rewards_receiver) {
        Err(err) => {
            let err = err.downcast::<ContractError>().unwrap();
            match err {
                ContractError::NoOrphanedRewards {} => {}
                unexpected_err => panic!("Unexpected error: {unexpected_err:?}"),
            }
        }
        _ => {}
    }
}

fn generate_cases() -> impl Strategy<Value = Vec<(Event, u64)>> {
    let lp_token_id_strategy = 0..MAX_POOLS;
    let reward_id_strategy = 0..MAX_REWARD_TOKENS;
    let percent_strategy = 10..=100u8;
    let time_strategy = 600..43200u64;
    let sender_id = 0..MAX_USERS;

    let events_strategy = prop_oneof![
        (
            sender_id.clone(),
            lp_token_id_strategy.clone(),
            percent_strategy.clone()
        )
            .prop_map(|(sender_id, lp_token_id, amount)| {
                Event::Deposit {
                    sender_id,
                    lp_token_id,
                    amount,
                }
            }),
        (
            sender_id.clone(),
            lp_token_id_strategy.clone(),
            percent_strategy.clone()
        )
            .prop_map(|(sender_id, lp_token_id, amount)| {
                Event::Withdraw {
                    sender_id,
                    lp_token_id,
                    amount,
                }
            }),
        sender_id.prop_map(|sender_id| { Event::Claim { sender_id } }),
        (
            lp_token_id_strategy.clone(),
            reward_id_strategy.clone(),
            1_000000..=1_000_000_000000u128,
            1..=MAX_PERIODS,
        )
            .prop_map(|(lp_token_id, reward_id, amount, duration)| {
                Event::Incentivize {
                    lp_token_id,
                    reward_id,
                    amount,
                    duration,
                }
            }),
        (
            prop::collection::vec(
                (lp_token_id_strategy.clone(), percent_strategy.clone()),
                1..=10
            ),
            500..=1_000000u128,
        )
            .prop_map(|(pools, tokens_per_second)| Event::SetupPools {
                pools: pools,
                tokens_per_second: tokens_per_second as u128,
            }),
        (lp_token_id_strategy.clone(), reward_id_strategy.clone()).prop_map(
            |(lp_token_id, reward_id)| Event::RemoveReward {
                lp_token_id,
                reward_id,
            }
        ),
    ];

    prop::collection::vec((events_strategy, time_strategy), 0..MAX_EVENTS)
}

proptest! {
    #[ignore]
    #[test]
    fn simulate(case in generate_cases()) {
        simulate_case(case);
    }
}

#[test]
fn single_test() {
    simulate_case(include!("test_case"))
}
