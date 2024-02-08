use std::str::FromStr;

use cosmwasm_std::{coin, coins, Decimal, Timestamp, Uint128};
use cw_multi_test::Executor;

use astroport::asset::{native_asset_info, AssetInfo, AssetInfoExt};
use astroport::incentives::{
    ExecuteMsg, IncentivizationFeeInfo, ScheduleResponse, EPOCHS_START, EPOCH_LENGTH,
    MAX_REWARD_TOKENS,
};
use astroport_incentives::error::ContractError;

use crate::helper::{assert_rewards, Helper, TestAddr};

mod helper;

#[test]
fn test_stake_unstake() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();

    let user = TestAddr::new("user");

    // ##### Check native LPs
    // TODO: build token factory based pair and test the lines below

    // let native_lp = native_asset_info("lp_token".to_string()).with_balance(1000u16);
    // helper.mint_coins(&user, vec![native_lp.as_coin().unwrap()]);
    //
    // helper.stake(&user, native_lp).unwrap();
    //
    // helper.unstake(&user, "lp_token", 500).unwrap();
    //
    // // Unstake more than staked
    // let err = helper.unstake(&user, "lp_token", 10000).unwrap_err();
    // assert_eq!(
    //     err.downcast::<ContractError>().unwrap(),
    //     ContractError::AmountExceedsBalance {
    //         available: 500u16.into(),
    //         withdraw_amount: 10000u16.into()
    //     }
    // );
    //
    // // Unstake non-existing LP token
    // let err = helper
    //     .unstake(&user, "non_existing_lp_token", 10000)
    //     .unwrap_err();
    // assert_eq!(
    //     err.downcast::<ContractError>().unwrap(),
    //     ContractError::PositionDoesntExist {
    //         user: user.to_string(),
    //         lp_token: "non_existing_lp_token".to_string()
    //     }
    // );
    //
    // helper.unstake(&user, "lp_token", 500).unwrap();

    // ##### Check cw20 LPs

    let asset_infos = [AssetInfo::native("uusd"), AssetInfo::native("ueur")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, false)
        .unwrap();

    let cw20_lp = AssetInfo::cw20(pair_info.liquidity_token.clone());
    let initial_lp_balance = cw20_lp.query_pool(&helper.app.wrap(), &user).unwrap();
    helper
        .stake(&user, cw20_lp.with_balance(initial_lp_balance))
        .unwrap();
    let lp_balance = cw20_lp.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(lp_balance.u128(), 0);

    // Unstake more than staked
    let err = helper
        .unstake(
            &user,
            pair_info.liquidity_token.as_str(),
            initial_lp_balance + Uint128::one(),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AmountExceedsBalance {
            available: initial_lp_balance,
            withdraw_amount: initial_lp_balance + Uint128::one()
        }
    );

    // Unstake half
    helper
        .unstake(
            &user,
            pair_info.liquidity_token.as_str(),
            initial_lp_balance.u128() / 2,
        )
        .unwrap();
    let lp_balance = cw20_lp.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(lp_balance.u128(), initial_lp_balance.u128() / 2);

    // Unstake the rest
    helper
        .unstake(
            &user,
            pair_info.liquidity_token.as_str(),
            initial_lp_balance.u128() / 2,
        )
        .unwrap();
    let lp_balance = cw20_lp.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(lp_balance, initial_lp_balance);
}

#[test]
fn test_claim_rewards() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();

    let mut pools = vec![
        ("uusd", "eur", "".to_string(), vec!["user1", "user2"], 100),
        ("uusd", "tokenA", "".to_string(), vec!["user1"], 50),
        ("uusd", "tokenB", "".to_string(), vec!["user2"], 50),
    ];

    let mut active_pools = vec![];
    for (token1, token2, lp_token, stakers, alloc_points) in pools.iter_mut() {
        let asset_infos = [AssetInfo::native(*token1), AssetInfo::native(*token2)];
        let pair_info = helper.create_pair(&asset_infos).unwrap();
        *lp_token = pair_info.liquidity_token.to_string();
        active_pools.push((pair_info.liquidity_token.to_string(), *alloc_points));

        let provide_assets = [
            asset_infos[0].with_balance(100000u64),
            asset_infos[1].with_balance(100000u64),
        ];
        // Owner provides liquidity first just make following calculations easier
        // since first depositor gets small cut of LP tokens
        helper
            .provide_liquidity(
                &owner,
                &provide_assets,
                &pair_info.contract_addr,
                false, // Owner doesn't stake in generator
            )
            .unwrap();

        for staker in stakers {
            let staker_addr = TestAddr::new(staker);

            // Pool doesn't exist in Generator yet
            let astro_before = astro.query_pool(&helper.app.wrap(), &staker_addr).unwrap();
            helper
                .claim_rewards(&staker_addr, vec![pair_info.liquidity_token.to_string()])
                .unwrap_err();
            let astro_after = astro.query_pool(&helper.app.wrap(), &staker_addr).unwrap();
            assert_eq!((astro_after - astro_before).u128(), 0);

            helper
                .provide_liquidity(
                    &staker_addr,
                    &provide_assets,
                    &pair_info.contract_addr,
                    true,
                )
                .unwrap();
        }
    }

    // Invalid active pools set
    let err = helper
        .setup_pools(vec![
            (TestAddr::new("pool1").to_string(), 1),
            (TestAddr::new("pool1").to_string(), 1),
        ])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicatedPoolFound {}
    );

    // Can't set 0 alloc point
    let err = helper
        .setup_pools(vec![
            (TestAddr::new("pool1").to_string(), 0),
            (TestAddr::new("pool2").to_string(), 1),
        ])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroAllocPoint {
            lp_token: TestAddr::new("pool1").to_string()
        }
    );

    // Only owner can execute operations below
    let err = helper
        .app
        .execute_contract(
            TestAddr::new("not_owner"),
            helper.generator.clone(),
            &ExecuteMsg::SetTokensPerSecond {
                amount: 1u128.into(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );
    let err = helper
        .app
        .execute_contract(
            TestAddr::new("not_owner"),
            helper.generator.clone(),
            &ExecuteMsg::SetupPools {
                pools: active_pools
                    .iter()
                    .map(|(pool, amount)| (pool.clone(), (*amount).into()))
                    .collect(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper.setup_pools(active_pools).unwrap();
    helper.set_tokens_per_second(1_000000).unwrap();

    // Block time still the same thus no rewards collected
    for (_, _, lp_token, stakers, _) in &pools {
        for staker in stakers {
            let staker_addr = TestAddr::new(staker);

            let pending = helper.query_pending_rewards(&staker_addr, &lp_token);
            let bal_before = helper.snapshot_balances(&staker_addr, &pending);

            helper
                .claim_rewards(&staker_addr, vec![lp_token.clone()])
                .unwrap();

            let bal_after = helper.snapshot_balances(&staker_addr, &pending);
            assert_rewards(&bal_before, &bal_after, &pending);
        }
    }

    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(5));

    let user1 = TestAddr::new("user1");
    let astro_before = astro.query_pool(&helper.app.wrap(), &user1).unwrap();
    let err = helper
        .claim_rewards(
            &user1,
            vec![
                pools[0].2.to_string(),
                pools[1].2.to_string(),
                pools[2].2.to_string(), // user1 doesn't have position in this pool and it should fail transaction
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PositionDoesntExist {
            user: user1.to_string(),
            lp_token: pools[2].2.to_string()
        }
    );

    helper
        .claim_rewards(&user1, vec![pools[0].2.to_string(), pools[1].2.to_string()])
        .unwrap();
    let astro_after = astro.query_pool(&helper.app.wrap(), &user1).unwrap();
    assert_eq!((astro_after - astro_before).u128(), 2_500000);

    let user2 = TestAddr::new("user2");
    let astro_before = astro.query_pool(&helper.app.wrap(), &user2).unwrap();
    helper
        .claim_rewards(&user2, vec![pools[0].2.to_string(), pools[2].2.to_string()])
        .unwrap();
    let astro_after = astro.query_pool(&helper.app.wrap(), &user2).unwrap();
    assert_eq!((astro_after - astro_before).u128(), 2_500000);
}

#[test]
fn test_incentives() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);
    helper.mint_assets(&bank, &[reward.clone()]);
    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_coin(&bank, &incentivization_fee);

    // Check general validation
    let err = helper
        .incentivize(&bank, &lp_token, schedule.clone(), &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IncentivizationFeeExpected {
            fee: incentivization_fee.to_string(),
            lp_token: lp_token.clone(),
            new_reward_token: reward_asset_info.to_string(),
        }
    );
    let err = helper
        .incentivize(&bank, &lp_token, schedule.clone(), &coins(1, "astro"))
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IncentivizationFeeExpected {
            fee: incentivization_fee.to_string(),
            lp_token: lp_token.clone(),
            new_reward_token: reward_asset_info.to_string(),
        }
    );
    let additional_random_funds = coin(1000u128, "uusd");
    helper.mint_coin(&bank, &additional_random_funds);
    let err = helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[additional_random_funds, incentivization_fee.clone()],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Supplied coins contain uusd that is not in the input asset vector"
    );

    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();

    // Check maker received incentivization fee
    let maker_amount = helper
        .app
        .wrap()
        .query_balance(TestAddr::new("maker"), &incentivization_fee.denom)
        .unwrap();
    assert_eq!(maker_amount.amount, incentivization_fee.amount);

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(internal_sch.next_epoch_start_ts)
    });

    // Iterate over 2 weeks by 1 day and claim rewards
    loop {
        if helper.app.block_info().time.seconds() > internal_sch.end_ts {
            break;
        }

        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        helper
            .app
            .update_block(|block| block.time = block.time.plus_seconds(86400));
    }

    let reward_balance = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    // A small amount of reward is lost due to rounding
    assert_eq!(reward_balance.u128(), 999_999986);

    // Claim after schedule ended doesn't do anything
    for _ in 0..5 {
        helper
            .app
            .update_block(|block| block.time = block.time.plus_seconds(86400));
        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
        let new_reward_balance = reward_asset_info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap();
        assert_eq!(new_reward_balance, reward_balance);
    }
}

#[test]
fn test_cw20_incentives() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");
    let reward_cw20 = helper.init_cw20("reward", None);
    let reward_asset_info = AssetInfo::cw20(reward_cw20);
    let reward = reward_asset_info.with_balance(1000_000000u128);
    helper.mint_assets(&bank, &[reward.clone()]);

    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_coin(&bank, &incentivization_fee);
    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(internal_sch.next_epoch_start_ts)
    });

    // Iterate over 2 weeks by 1 day and claim rewards
    loop {
        if helper.app.block_info().time.seconds() > internal_sch.end_ts {
            break;
        }

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        helper
            .app
            .update_block(|block| block.time = block.time.plus_seconds(86400));
    }

    let reward_balance = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    // A small amount of reward is lost due to rounding
    assert_eq!(reward_balance.u128(), 999_999986);

    // Claiming after schedule ended doesn't do anything
    for _ in 0..5 {
        helper
            .app
            .update_block(|block| block.time = block.time.plus_seconds(86400));

        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        let new_reward_balance = reward_asset_info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap();
        assert_eq!(new_reward_balance, reward_balance);
    }
}

#[test]
fn test_multiple_schedules_same_reward() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    // Incentivize with ASTRO
    helper.setup_pools(vec![(lp_token.clone(), 100)]).unwrap();
    helper.set_tokens_per_second(100).unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);

    // Create multiple overlapping schedules with the same reward token starting right away
    let schedules: Vec<_> = (1..=5)
        .into_iter()
        .map(|i| helper.create_schedule(&reward, i).unwrap())
        .collect();
    for (ind, (schedule, _)) in schedules.iter().enumerate() {
        helper.mint_assets(&bank, &[reward.clone()]);
        if ind == 0 {
            // attach incentivization fee on the first schedule
            helper.mint_coin(&bank, &incentivization_fee);
            helper
                .incentivize(
                    &bank,
                    &lp_token,
                    schedule.clone(),
                    &[incentivization_fee.clone()],
                )
                .unwrap();
        } else {
            helper
                .incentivize(&bank, &lp_token, schedule.clone(), &[])
                .unwrap();
        }
    }

    let time_before_claims = helper.app.block_info().time.seconds();

    // Rewards started right away
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(86400));
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
    let reward_balance = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    assert_eq!(reward_balance.u128(), 207_189296);
    // And received ASTRO rewards
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(astro_reward_balance.u128(), 86400 * 100);

    // Iterate till the end of the longest schedule by 1 day and claim rewards
    loop {
        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        if helper.app.block_info().time.seconds() > schedules.last().cloned().unwrap().1.end_ts {
            break;
        } else {
            helper.next_block(86400)
        }
    }

    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
    let reward_balance = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    // A small amount of reward is lost due to rounding
    assert_eq!(reward_balance.u128(), 4999_999972);

    let time_now = helper.app.block_info().time.seconds();
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(
        astro_reward_balance.u128(),
        u128::from(time_now - time_before_claims) * 100
    );
}

#[test]
fn test_multiple_schedules_different_reward() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    // Incentivize with ASTRO
    helper.setup_pools(vec![(lp_token.clone(), 100)]).unwrap();
    helper.set_tokens_per_second(100).unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");

    let schedules: Vec<_> = (1..=MAX_REWARD_TOKENS)
        .into_iter()
        .map(|i| {
            let reward_asset_info = AssetInfo::native(format!("reward{i}"));
            let reward = reward_asset_info.with_balance(1000_000000u128);
            helper.create_schedule(&reward, 2).unwrap()
        })
        .collect();
    // Create multiple schedules with different rewards (starts on the next week)
    for (schedule, _) in &schedules {
        helper.mint_assets(&bank, &[schedule.reward.clone()]);
        helper.mint_coin(&bank, &incentivization_fee);
        helper
            .incentivize(
                &bank,
                &lp_token,
                schedule.clone(),
                &[incentivization_fee.clone()],
            )
            .unwrap();
    }

    // Can't incentivize with one more reward token
    let reward_asset_info = AssetInfo::native(format!("reward{}", MAX_REWARD_TOKENS + 1));
    let reward = reward_asset_info.with_balance(1000_000000u128);
    let (schedule, _) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_assets(&bank, &[schedule.reward.clone()]);
    helper.mint_coin(&bank, &incentivization_fee);
    let err = helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::TooManyRewardTokens {
            lp_token: lp_token.clone()
        }
    );

    let time_before_claims = helper.app.block_info().time.seconds();

    // Rewards started right away
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(86400));
    for (schedule, _) in &schedules {
        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
        let reward_balance = schedule
            .reward
            .info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap();
        assert_eq!(reward_balance.u128(), 47_629547);
    }
    // And received ASTRO rewards
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(astro_reward_balance.u128(), 86400 * 100);

    // Iterate till the end of the longest schedule by 1 day and claim rewards
    loop {
        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        if helper.app.block_info().time.seconds() > schedules.last().cloned().unwrap().1.end_ts {
            break;
        } else {
            helper
                .app
                .update_block(|block| block.time = block.time.plus_seconds(86400));
        }
    }

    for (schedule, _) in &schedules {
        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
        let reward_balance = schedule
            .reward
            .info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap();
        // Total amount is a bit off because of rounding due to Decimal type
        assert_eq!(
            reward_balance.u128(),
            999_999980,
            "Balance for {} is wrong",
            schedule.reward.info
        );
    }

    let time_now = helper.app.block_info().time.seconds();
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(
        astro_reward_balance.u128(),
        u128::from(time_now - time_before_claims) * 100
    );
}

#[test]
fn test_claim_between_different_periods() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    // External incentives
    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);

    // Create multiple overlapping schedules with the same reward token starting right away
    let schedules: Vec<_> = (1..=2)
        .into_iter()
        .map(|i| helper.create_schedule(&reward, i).unwrap())
        .collect();
    for (ind, (schedule, _)) in schedules.iter().enumerate() {
        helper.mint_assets(&bank, &[reward.clone()]);
        if ind == 0 {
            // attach incentivization fee on the first schedule
            helper.mint_coin(&bank, &incentivization_fee);
            helper
                .incentivize(
                    &bank,
                    &lp_token,
                    schedule.clone(),
                    &[incentivization_fee.clone()],
                )
                .unwrap();
        } else {
            helper
                .incentivize(&bank, &lp_token, schedule.clone(), &[])
                .unwrap();
        }
    }

    // Incentivize with ASTRO
    helper.setup_pools(vec![(lp_token.clone(), 100)]).unwrap();
    helper.set_tokens_per_second(100).unwrap();

    let time_before_claims = helper.app.block_info().time.seconds();

    // Shift time by 15 days
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(15 * 86400));

    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    let time_now = helper.app.block_info().time.seconds();
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(
        astro_reward_balance.u128(),
        u128::from(time_now - time_before_claims) * 100
    );
}

#[test]
fn test_astro_external_reward() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(EPOCHS_START + EPOCH_LENGTH));

    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    // Incentivize with ASTRO
    helper.setup_pools(vec![(lp_token.clone(), 100)]).unwrap();
    helper.set_tokens_per_second(100).unwrap();

    let active_pools = helper.active_pools();
    assert_eq!(active_pools, vec![(lp_token.clone(), 100u8.into())]);

    // Setup external rewards: 2 equal external ASTRO rewards that must be summed up
    let bank = TestAddr::new("bank");
    let reward = astro.with_balance(2u128 * 7 * 86400 * 25); // 25 uastro per second
    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_assets(&bank, &[reward.clone()]);
    helper.mint_coin(&bank, &incentivization_fee);
    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();
    // 2nd schedule doesn't require incentivization fee
    helper.mint_assets(&bank, &[reward.clone()]);
    helper
        .incentivize(&bank, &lp_token, schedule.clone(), &[])
        .unwrap();

    // Prepare user's liquidity
    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let time_before_claims = helper.app.block_info().time.seconds();

    helper.next_block(86400);

    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(astro_reward_balance.u128(), 86400 * (100 + 50));

    // Iterate till the end of the schedule by 1 day and claim rewards
    loop {
        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        if helper.app.block_info().time.seconds() > internal_sch.end_ts {
            break;
        } else {
            helper.next_block(86400);
        }
    }

    let time_now = helper.app.block_info().time.seconds();
    let astro_reward_balance = astro.query_pool(&helper.app.wrap(), &user).unwrap();
    assert_eq!(
        astro_reward_balance.u128(),
        u128::from(time_now - time_before_claims) * 100 // protocol rewards
            + u128::from(internal_sch.end_ts - internal_sch.next_epoch_start_ts) * 50 // external rewards
    );
}

#[test]
fn test_blocked_tokens() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let guardian = TestAddr::new("guardian");

    let tokens = [
        AssetInfo::native("usd"),
        AssetInfo::native("foo"),
        AssetInfo::native("blk"),
        AssetInfo::native("bar"),
    ];
    let norm_pair1_info = helper
        .create_pair(&[tokens[0].clone(), tokens[1].clone()])
        .unwrap();
    let norm_pair2_info = helper
        .create_pair(&[tokens[0].clone(), tokens[3].clone()])
        .unwrap();

    // Check general validation
    let err = helper
        .block_tokens(&guardian, &[astro.clone()])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Generic error: Blocking ASTRO token {} is prohibited",
            &astro
        )
    );
    let err = helper
        .block_tokens(&TestAddr::new("random"), &[tokens[2].clone()])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );
    let err = helper
        .unblock_tokens(&owner, &[tokens[2].clone()])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Generic error: Token {} wasn't found in the blocked list",
            &tokens[2]
        )
    );

    let err = helper
        .update_blocklist(&owner, &[tokens[2].clone(), tokens[2].clone()], &[])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Duplicated tokens found"
    );

    let err = helper
        .update_blocklist(&owner, &[], &[tokens[0].clone(), tokens[0].clone()])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Duplicated tokens found"
    );

    let err = helper
        .update_blocklist(
            &owner,
            &[tokens[0].clone()],
            &[tokens[0].clone(), tokens[1].clone()],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Duplicated tokens found"
    );

    // Block 'blk' token
    helper.block_tokens(&owner, &[tokens[2].clone()]).unwrap();

    let blocked = helper.blocked_tokens();
    assert_eq!(blocked[0], tokens[2]);

    let err = helper
        .block_tokens(&owner, &[tokens[2].clone()])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Generic error: Token {} is already in the blocked list",
            &tokens[2]
        )
    );

    // Create pair with blocked token 'blk' and stake in Generator.
    // Generator should allow it.
    let blk_pair_info = helper
        .create_pair(&[tokens[0].clone(), tokens[2].clone()])
        .unwrap();

    // Try to add ASTRO emissions to the 'blk' pair
    let err = helper
        .setup_pools(vec![
            (norm_pair1_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
            (blk_pair_info.liquidity_token.to_string(), 1),
        ])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::BlockedToken {
            token: tokens[2].to_string()
        }
    );

    // Activate allowed pairs
    helper
        .setup_pools(vec![
            (norm_pair1_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
        ])
        .unwrap();
    helper.set_tokens_per_second(100).unwrap();

    helper.next_block(1000);

    // Unblock 'blk' token and remove norm_pair1 from active set
    helper.unblock_tokens(&owner, &[tokens[2].clone()]).unwrap();
    helper
        .setup_pools(vec![
            (blk_pair_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
        ])
        .unwrap();

    // For simplicity we have no stakers in this test. However, all rewards are accrued in 'orphaned_rewards'
    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // 50 astro * 1000 passed seconds
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000);
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 0); // This pair was just incentivized in this block

    helper.next_block(1000);

    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // deactivated pool didn't get anything
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 2000);
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000);

    // Block poor 'blk' token again. It should automatically deactivate blk_pair
    helper
        .block_tokens(&guardian, &[tokens[2].clone()])
        .unwrap();

    helper.next_block(1000);

    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // deactivated pool didn't get anything
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(
        reward_info[0].orphaned.to_uint_floor().u128(),
        50 * 2000 + 100 * 1000
    ); // this pools is the only active atm
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // deactivated blk pair didn't get anything
}

#[test]
fn test_blocked_pair_types() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();

    let tokens = [
        AssetInfo::native("usd"),
        AssetInfo::native("foo"),
        AssetInfo::native("bar"),
    ];
    let norm_pair1_info = helper
        .create_pair(&[tokens[0].clone(), tokens[1].clone()])
        .unwrap();
    let norm_pair2_info = helper
        .create_pair(&[tokens[0].clone(), tokens[2].clone()])
        .unwrap();
    let blk_pair_info = helper.create_stable_pair(&[tokens[1].clone(), tokens[2].clone()]);

    // Activate all pairs. blk pair is not blocked yet
    helper
        .setup_pools(vec![
            (norm_pair1_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
            (blk_pair_info.liquidity_token.to_string(), 1),
        ])
        .unwrap();
    helper.set_tokens_per_second(150).unwrap();

    helper.next_block(1000);

    // Block 'blk' pair
    helper
        .block_pair_type(&owner, blk_pair_info.pair_type.clone())
        .unwrap();

    // For simplicity we have no stakers in this test. However, all rewards are accrued in 'orphaned_rewards'
    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // 50 astro * 1000 passed seconds
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000);
    // Although this pair is blocked, it still gets rewards until manually deactivated
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000);

    // Deactivate 'blk' pair
    helper.deactivate_blocked().unwrap();

    helper.next_block(1000);

    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(
        reward_info[0].orphaned.to_uint_floor().u128(),
        50 * 1000 + 75 * 1000
    );
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(
        reward_info[0].orphaned.to_uint_floor().u128(),
        50 * 1000 + 75 * 1000
    );
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // deactivated blk pair didn't get anything

    // Next time setup pool won't allow to activate 'blk' pair
    let err = helper
        .setup_pools(vec![
            (norm_pair1_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
            (blk_pair_info.liquidity_token.to_string(), 1),
        ])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::BlockedPairType {
            pair_type: blk_pair_info.pair_type.clone()
        }
    );

    // All subsequent deactivate_blocked calls will do nothing
    helper.deactivate_blocked().unwrap();

    // Lets check factory deactivation logic
    // Only factory can deactivate pair
    let err = helper
        .deactivate_pool(&owner, norm_pair1_info.liquidity_token.as_str())
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    // Deactivate norm_pair1_info by its asset infos
    helper
        .deactivate_pool_full_flow(&[tokens[0].clone(), tokens[1].clone()])
        .unwrap();

    let err = helper
        .setup_pools(vec![
            (norm_pair1_info.liquidity_token.to_string(), 1),
            (norm_pair2_info.liquidity_token.to_string(), 1),
        ])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Generic error: The pair is not registered: {}-{}",
            &tokens[0], &tokens[1]
        )
    );

    helper.next_block(1000);

    let reward_info = helper.query_reward_info(norm_pair1_info.liquidity_token.as_str());
    assert_eq!(
        reward_info[0].orphaned.to_uint_floor().u128(),
        50 * 1000 + 75 * 1000 // deactivated pool gets nothing
    );
    let reward_info = helper.query_reward_info(norm_pair2_info.liquidity_token.as_str());
    assert_eq!(
        reward_info[0].orphaned.to_uint_floor().u128(),
        50 * 1000 + 75 * 1000 + 150 * 1000
    );
    let reward_info = helper.query_reward_info(blk_pair_info.liquidity_token.as_str());
    assert_eq!(reward_info[0].orphaned.to_uint_floor().u128(), 50 * 1000); // deactivated blk pair still gets nothing
}

#[test]
fn test_incentives_with_blocked() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();

    // Try to incentivize with blocked token
    let bank = TestAddr::new("bank");
    let blocked_token = AssetInfo::native("blocked_reward");
    helper
        .block_tokens(&owner, &[blocked_token.clone()])
        .unwrap();
    let reward = blocked_token.with_balance(1000_000000u128);
    helper.mint_assets(&bank, &[reward.clone()]);

    let (schedule, _) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_coin(&bank, &incentivization_fee);
    let err = helper
        .incentivize(
            &bank,
            pair_info.liquidity_token.as_str(),
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::BlockedToken {
            token: blocked_token.to_string()
        }
    );
}

#[test]
fn test_remove_rewards() {
    let astro = native_asset_info("astro".to_string());

    let mut helper = Helper::new("owner", &astro).unwrap();
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(EPOCHS_START + EPOCH_LENGTH));

    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);
    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();

    helper.mint_assets(&bank, &[reward.clone()]);
    helper.mint_coin(&bank, &incentivization_fee);

    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(internal_sch.next_epoch_start_ts)
    });

    // 5 days
    for _ in 0..5 {
        helper.next_block(86400);

        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);
    }

    // Assume 1 day passed and then reward gets deregistered
    helper.next_block(86400);

    let receiver = TestAddr::new("receiver");

    // Only owner is able to remove reward
    let err = helper
        .remove_reward(
            &TestAddr::new("random"),
            &lp_token,
            &reward_asset_info.to_string(),
            false,
            &receiver,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper
        .remove_reward(
            &owner,
            &lp_token,
            &reward_asset_info.to_string(),
            false,
            &receiver,
        )
        .unwrap();

    // User must be allowed to claim rewards for the last 1 day
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    let outstanding = reward_asset_info
        .query_pool(&helper.app.wrap(), &receiver)
        .unwrap()
        .u128();
    let claimed = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap()
        .u128();
    assert_eq!(outstanding, 571_428571); // ~ 8 * 71428571 (8 days left)
    assert_eq!(claimed, 428_571426); // // claimed 6 days in a row ~ 6 * 71428571
    assert_eq!(outstanding + claimed, 999_999997); // ~ initial reward amount i.e. 1000_000000
}

#[test]
fn test_long_unclaimed_rewards() {
    let astro = native_asset_info("astro".to_string());

    let mut helper = Helper::new("owner", &astro).unwrap();
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(EPOCHS_START + EPOCH_LENGTH));

    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");

    let schedules: Vec<_> = (1..=2)
        .into_iter()
        .map(|i| {
            let reward_asset_info = AssetInfo::native(format!("reward{i}"));
            let reward = reward_asset_info.with_balance(50_000_000000u128);
            // Create 2 schedules with different duration
            [
                helper.create_schedule(&reward, 20).unwrap(),
                helper.create_schedule(&reward, 10).unwrap(),
            ]
        })
        .flatten()
        .collect();
    let max_end = schedules.iter().map(|(_, sch)| sch.end_ts).max().unwrap();
    // Create multiple schedules with different rewards (starts on the next week)
    for (ind, (schedule, _)) in schedules.iter().enumerate() {
        helper.mint_assets(&bank, &[schedule.reward.clone()]);
        let mut attach_funds = vec![];
        if ind % 2 == 0 {
            helper.mint_coin(&bank, &incentivization_fee);
            attach_funds = vec![incentivization_fee.clone()]
        }
        helper
            .incentivize(&bank, &lp_token, schedule.clone(), &attach_funds)
            .unwrap();
    }

    // Start from the starting point and jump over 5 weeks
    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(schedules[0].1.next_epoch_start_ts + 86400 * 7 * 5);
    });

    // Deregister reward1
    let receiver = TestAddr::new("receiver");
    helper
        .remove_reward(
            &owner,
            &lp_token,
            &schedules[0].0.reward.info.to_string(),
            false,
            &receiver,
        )
        .unwrap();
    let deregister_amount = schedules[0]
        .0
        .reward
        .info
        .query_pool(&helper.app.wrap(), &receiver)
        .unwrap()
        .u128();
    assert_eq!(deregister_amount, 62499_999999);

    // Iterate till the end of the longest schedule by 1 day and claim rewards
    loop {
        let pending = helper.query_pending_rewards(&user, &lp_token);
        let bal_before = helper.snapshot_balances(&user, &pending);

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        let bal_after = helper.snapshot_balances(&user, &pending);
        assert_rewards(&bal_before, &bal_after, &pending);

        if helper.app.block_info().time.seconds() > max_end {
            break;
        } else {
            helper.next_block(86400);
        }
    }

    let claimed_reward1 = schedules[0]
        .0
        .reward
        .info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap()
        .u128();
    assert_eq!(deregister_amount + claimed_reward1, 99999_999998);

    for (schedule, _) in schedules.iter().skip(2) {
        let bal = schedule
            .reward
            .info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap()
            .u128();
        assert_eq!(bal, 99_999_999974); // All rewards are claimed
    }
}

#[test]
fn test_queries() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // doesnt stake
        )
        .unwrap();

    for i in 0..10 {
        let user = TestAddr::new(&format!("user_{i}"));
        helper
            .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
            .unwrap();
    }

    let user = TestAddr::new("user_1");

    assert_eq!(helper.query_deposit(&lp_token, &user).unwrap(), 100000);

    let random = TestAddr::new("random");
    let amount = helper.query_deposit(&lp_token, &random).unwrap();
    assert_eq!(amount, 0);

    let amount = helper.query_deposit(random.as_str(), &user).unwrap();
    assert_eq!(amount, 0);

    // This Lp doesn't exist
    helper.pool_info(random.as_str()).unwrap_err();

    let pool_info = helper.pool_info(&lp_token).unwrap();
    assert_eq!(pool_info.rewards, []);
    assert_eq!(pool_info.total_lp.u128(), 1_000_000); // 100_000 per user
    assert_eq!(
        pool_info.last_update_ts,
        helper.app.block_info().time.seconds()
    );

    let stakers = helper.pool_stakers(&lp_token, None, None);
    assert_eq!(
        stakers[5],
        ("wasm1_user_5".to_string(), Uint128::from(100_000u128))
    );
    let total = stakers
        .iter()
        .fold(Uint128::zero(), |acc, (_, bal)| acc + bal);
    assert_eq!(total, pool_info.total_lp);

    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);

    // Create multiple overlapping schedules with the same reward token starting right away
    let schedules: Vec<_> = (1..=5)
        .into_iter()
        .map(|i| helper.create_schedule(&reward, i).unwrap())
        .collect();
    for (ind, (schedule, _)) in schedules.iter().enumerate() {
        helper.mint_assets(&bank, &[reward.clone()]);
        if ind == 0 {
            // attach incentivization fee on the first schedule
            helper.mint_coin(&bank, &incentivization_fee);
            helper
                .incentivize(
                    &bank,
                    &lp_token,
                    schedule.clone(),
                    &[incentivization_fee.clone()],
                )
                .unwrap();
        } else {
            helper
                .incentivize(&bank, &lp_token, schedule.clone(), &[])
                .unwrap();
        }
    }

    let res = helper
        .query_ext_reward_schedules(&lp_token, &reward_asset_info, None, None)
        .unwrap();
    assert_eq!(
        res,
        [
            ScheduleResponse {
                rps: Decimal::from_str("2398.02426572720408957").unwrap(),
                start_ts: 1696810000,
                end_ts: 1698019200,
            },
            ScheduleResponse {
                rps: Decimal::from_str("1571.031212468851459733").unwrap(),
                start_ts: 1698019200,
                end_ts: 1698624000,
            },
            ScheduleResponse {
                rps: Decimal::from_str("1019.76329626157472324").unwrap(),
                start_ts: 1698624000,
                end_ts: 1699228800,
            },
            ScheduleResponse {
                rps: Decimal::from_str("606.335150073382231096").unwrap(),
                start_ts: 1699228800,
                end_ts: 1699833600,
            },
            ScheduleResponse {
                rps: Decimal::from_str("275.603571822290816888").unwrap(),
                start_ts: 1699833600,
                end_ts: 1700438400,
            },
        ]
    );
    let res = helper
        .query_ext_reward_schedules(&lp_token, &reward_asset_info, None, Some(1))
        .unwrap();
    assert_eq!(res.len(), 1);
    let res = helper
        .query_ext_reward_schedules(&lp_token, &reward_asset_info, Some(1699228800), None)
        .unwrap();
    assert_eq!(res.len(), 2);

    let pools = helper.all_pools();
    assert_eq!(pools, vec![lp_token.clone()]);
}

#[test]
fn test_update_config() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();

    let new_vesting = TestAddr::new("new_vesting");
    let new_generator_controller = TestAddr::new("new_generator_controller");
    let new_guardian = TestAddr::new("new_guardian");
    let new_incentivization_fee_info = IncentivizationFeeInfo {
        fee_receiver: TestAddr::new("new_fee_receiver"),
        fee: coin(1000, "uusd"),
    };

    let msg = ExecuteMsg::UpdateConfig {
        vesting_contract: Some(new_vesting.to_string()),
        generator_controller: Some(new_generator_controller.to_string()),
        guardian: Some(new_guardian.to_string()),
        incentivization_fee_info: Some(new_incentivization_fee_info.clone()),
    };

    let err = helper
        .app
        .execute_contract(TestAddr::new("random"), helper.generator.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper
        .app
        .execute_contract(helper.owner.clone(), helper.generator.clone(), &msg, &[])
        .unwrap();

    let config = helper.query_config();
    assert_eq!(config.vesting_contract, new_vesting);
    assert_eq!(
        config.generator_controller.unwrap(),
        new_generator_controller
    );
    assert_eq!(config.guardian.unwrap(), new_guardian);
    assert_eq!(
        config.incentivization_fee_info.unwrap(),
        new_incentivization_fee_info
    );
}

#[test]
fn test_change_ownership() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();

    let new_owner = TestAddr::new("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            TestAddr::new("not_owner"),
            helper.generator.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.generator.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    helper
        .app
        .execute_contract(helper.owner.clone(), helper.generator.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            TestAddr::new("invalid_addr"),
            helper.generator.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.generator.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Claim ownership
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.generator.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner again
    helper
        .app
        .execute_contract(helper.owner.clone(), helper.generator.clone(), &msg, &[])
        .unwrap();
    helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.generator.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap();

    assert_eq!(helper.query_config().owner.to_string(), new_owner)
}

#[test]
fn test_incentive_without_funds() {
    let astro = native_asset_info("astro".to_string());
    let usdc = native_asset_info("usdc".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();
    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just make following calculations easier // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();
    let bank = TestAddr::new("bank");
    let reward_asset_info = usdc.clone();
    let reward = reward_asset_info.with_balance(1000_000000u128);
    helper.mint_assets(&bank, &[reward.clone()]);
    let (schedule, _) = helper.create_schedule(&reward, 2).unwrap();
    let incentivization_fee = helper.incentivization_fee.clone();
    helper.mint_coin(&bank, &incentivization_fee);
    // add reward
    let err = helper
        .app
        .execute_contract(
            bank.clone(),
            helper.generator.clone(),
            &ExecuteMsg::Incentivize {
                lp_token: lp_token.to_string(),
                schedule,
            },
            &[incentivization_fee], // only send incentivization fee without reward
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Generic error: Native token balance mismatch between the argument (1000000000usdc) and the transferred (0usdc)")
}

#[test]
fn test_claim_excess_rewards() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let mut pools = vec![
        ("uusd", "eur", "".to_string(), vec!["user1", "user2"], 100),
        ("uusd", "tokenA", "".to_string(), vec!["user1"], 50),
        ("uusd", "tokenB", "".to_string(), vec!["user2"], 50),
    ];
    let mut active_pools = vec![];
    for (token1, token2, lp_token, stakers, alloc_points) in pools.iter_mut() {
        let asset_infos = [AssetInfo::native(*token1), AssetInfo::native(*token2)];
        let pair_info = helper.create_pair(&asset_infos).unwrap();
        *lp_token = pair_info.liquidity_token.to_string();
        active_pools.push((pair_info.liquidity_token.to_string(), *alloc_points));
        let provide_assets = [
            asset_infos[0].with_balance(100000u64),
            asset_infos[1].with_balance(100000u64),
        ];
        // Owner provides liquidity first just to make following calculations easier
        // since first depositor gets small cut of LP tokens
        helper
            .provide_liquidity(
                &owner,
                &provide_assets,
                &pair_info.contract_addr,
                false, // Owner doesn't stake in generator
            )
            .unwrap();

        for staker in stakers {
            let staker_addr = TestAddr::new(staker);
            // Pool doesn't exist in Generator yet
            let astro_before = astro.query_pool(&helper.app.wrap(), &staker_addr).unwrap();
            helper
                .claim_rewards(
                    &staker_addr,
                    vec![
                        pair_info.liquidity_token.to_string(),
                        pair_info.liquidity_token.to_string(),
                    ],
                )
                .unwrap_err();
            let astro_after = astro.query_pool(&helper.app.wrap(), &staker_addr).unwrap();
            assert_eq!((astro_after - astro_before).u128(), 0);

            helper
                .provide_liquidity(
                    &staker_addr,
                    &provide_assets,
                    &pair_info.contract_addr,
                    true,
                )
                .unwrap();
        }
    }

    helper.setup_pools(active_pools).unwrap();
    helper.set_tokens_per_second(1_000000).unwrap();
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(5));
    let user1 = TestAddr::new("user1");
    let astro_before = astro.query_pool(&helper.app.wrap(), &user1).unwrap();
    let err = helper
        .claim_rewards(
            &user1,
            vec![
                pools[0].2.to_string(),
                pools[1].2.to_string(),
                pools[0].2.to_string(),
                pools[1].2.to_string(),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicatedPoolFound {}
    );

    helper
        .claim_rewards(&user1, vec![pools[0].2.to_string(), pools[1].2.to_string()])
        .unwrap();
    let astro_after = astro.query_pool(&helper.app.wrap(), &user1).unwrap();
    assert_eq!((astro_after - astro_before).u128(), 2_500000);
}

#[test]
fn test_user_claim_less() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();
    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];

    // Owner provides liquidity first just to make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");
    let reward_asset_info = AssetInfo::native("reward");
    let reward = reward_asset_info.with_balance(1000_000000u128);

    // create reward schedule
    helper.mint_assets(&bank, &[reward.clone()]);
    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_coin(&bank, &incentivization_fee);
    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(internal_sch.next_epoch_start_ts)
    });

    // user claim, sets user index
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    // finish 1st schedule, reward goes to FINISHED_REWARD_INDEXES
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(internal_sch.end_ts + 1));

    // create reward schedule again
    helper.mint_assets(&bank, &[reward.clone()]);
    let (schedule, internal_sch) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_coin(&bank, &incentivization_fee);
    helper
        .incentivize(
            &bank,
            &lp_token,
            schedule.clone(),
            &[incentivization_fee.clone()],
        )
        .unwrap();

    // few seconds before schedule finishes
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(internal_sch.end_ts - 1));

    // user claim rewards as (global index - user index), which is incorrect
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    // finish 2nd schedule
    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(internal_sch.end_ts + 1));

    // user claim all rewards
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    // check user rewards
    let new_reward_balance = reward_asset_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();

    assert_eq!(
        new_reward_balance.u128(),
        (reward.amount + reward.amount).u128() - 2 // rounding error
    );
}

#[test]
fn test_broken_cw20_incentives() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let owner = helper.owner.clone();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    // Owner provides liquidity first just make following calculations easier
    // since first depositor gets small cut of LP tokens
    helper
        .provide_liquidity(
            &owner,
            &provide_assets,
            &pair_info.contract_addr,
            false, // Owner doesn't stake in generator
        )
        .unwrap();

    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    let bank = TestAddr::new("bank");

    let schedules: Vec<_> = (1..=2)
        .into_iter()
        .map(|i| {
            let reward_asset_info = if i == 1 {
                AssetInfo::native(format!("reward{i}"))
            } else {
                let reward_cw20 = helper.init_broken_cw20("reward", None);
                AssetInfo::cw20(reward_cw20)
            };

            let reward = reward_asset_info.with_balance(1000_000000u128);
            helper.create_schedule(&reward, 1).unwrap()
        })
        .collect();

    // Create multiple schedules with different rewards
    for (schedule, _) in &schedules {
        helper.mint_assets(&bank, &[schedule.reward.clone()]);
        helper.mint_coin(&bank, &incentivization_fee);
        helper
            .incentivize(
                &bank,
                &lp_token,
                schedule.clone(),
                &[incentivization_fee.clone()],
            )
            .unwrap();
    }

    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(schedules[0].1.next_epoch_start_ts)
    });

    // Iterate by 1 day and claim rewards
    loop {
        if helper.app.block_info().time.seconds() > schedules[0].1.end_ts {
            break;
        }

        helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

        helper
            .app
            .update_block(|block| block.time = block.time.plus_seconds(86400));
    }

    // Valid native coin reward was accrued properly
    let valid_reward_balance = schedules[0]
        .1
        .reward_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    assert_eq!(valid_reward_balance.u128(), 999_999994);

    // Broken cw20 reward was not accrued because incentives contract simply ignores it
    let broken_reward_balance = schedules[1]
        .1
        .reward_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    assert_eq!(broken_reward_balance.u128(), 0);
}

#[test]
fn test_factory_deregisters_any_pool() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let asset_infos = &[AssetInfo::native("usd"), AssetInfo::native("foo")];

    // factory contract create pair
    helper.create_pair(asset_infos).unwrap();
    // ensure pair created
    let pair_info = helper.query_pair_info(asset_infos);
    assert_eq!(pair_info.asset_infos, asset_infos);

    // Incentives contract doesn't have such pool yet but it doesn't block deregistration
    helper.deactivate_pool_full_flow(asset_infos).unwrap();
}

#[test]
fn test_orphaned_rewards() {
    let astro = native_asset_info("astro".to_string());
    let mut helper = Helper::new("owner", &astro).unwrap();
    let incentivization_fee = helper.incentivization_fee.clone();

    let asset_infos = [AssetInfo::native("foo"), AssetInfo::native("bar")];
    let pair_info = helper.create_pair(&asset_infos).unwrap();
    let lp_token = pair_info.liquidity_token.to_string();

    let bank = TestAddr::new("bank");

    let schedules: Vec<_> = (1..=(MAX_REWARD_TOKENS - 1))
        .into_iter()
        .map(|i| {
            let reward_asset_info = AssetInfo::native(format!("reward{i}"));
            let reward = reward_asset_info.with_balance(1000_000000u128);
            helper.create_schedule(&reward, 2).unwrap()
        })
        .collect();
    // Create multiple schedules with different rewards
    for (schedule, _) in &schedules {
        helper.mint_assets(&bank, &[schedule.reward.clone()]);
        helper.mint_coin(&bank, &incentivization_fee);
        helper
            .incentivize(
                &bank,
                &lp_token,
                schedule.clone(),
                &[incentivization_fee.clone()],
            )
            .unwrap();
    }

    // Timing out all schedules. nobody stakes LP tokens, rewards become orphaned
    helper.app.update_block(|block| {
        block.time = Timestamp::from_seconds(schedules.last().cloned().unwrap().1.end_ts + 1)
    });

    let orph_receiver = TestAddr::new("orphaned_rewards_receiver");

    // Check that there are still no orphaned rewards to claim
    let err = helper
        .claim_orphaned_rewards(None, &orph_receiver)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoOrphanedRewards {}
    );

    // Add one more reward thus triggering finished rewards cleanup
    let reward =
        AssetInfo::native(format!("reward{MAX_REWARD_TOKENS}")).with_balance(1000_000000u128);
    let (new_schedule, int_new_schedule) = helper.create_schedule(&reward, 2).unwrap();
    helper.mint_assets(&bank, &[reward]);
    helper.mint_coin(&bank, &incentivization_fee);
    helper
        .incentivize(
            &bank,
            &lp_token,
            new_schedule,
            &[incentivization_fee.clone()],
        )
        .unwrap();

    // Provide to check that user is only eligible for the last added reward
    let provide_assets = [
        asset_infos[0].with_balance(100000u64),
        asset_infos[1].with_balance(100000u64),
    ];
    let user = TestAddr::new("user");
    helper
        .provide_liquidity(&user, &provide_assets, &pair_info.contract_addr, true)
        .unwrap();

    helper
        .app
        .update_block(|block| block.time = Timestamp::from_seconds(int_new_schedule.end_ts + 1));

    // Claim rewards and assert user only gets reward5
    helper.claim_rewards(&user, vec![lp_token.clone()]).unwrap();

    for (schedule, _) in &schedules {
        let reward_balance = schedule
            .reward
            .info
            .query_pool(&helper.app.wrap(), &user)
            .unwrap();
        assert_eq!(reward_balance.u128(), 0);
    }

    let reward_balance = int_new_schedule
        .reward_info
        .query_pool(&helper.app.wrap(), &user)
        .unwrap();
    assert_eq!(reward_balance.u128(), 999_999999);

    // Owner claims first orphaned rewards
    helper
        .claim_orphaned_rewards(Some(1), &orph_receiver)
        .unwrap();

    // Owner claims all orphaned rewards
    helper.claim_orphaned_rewards(None, &orph_receiver).unwrap();

    for (schedule, _) in &schedules {
        let reward_balance = schedule
            .reward
            .info
            .query_pool(&helper.app.wrap(), &orph_receiver)
            .unwrap();
        assert_eq!(reward_balance.u128(), 999999999);
    }

    // Try to claim again
    let err = helper
        .claim_orphaned_rewards(None, &orph_receiver)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoOrphanedRewards {}
    );
}
