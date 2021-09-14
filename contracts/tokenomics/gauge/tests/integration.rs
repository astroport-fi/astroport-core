use astroport::{
    gauge::{
        ExecuteMsg as GaugeExecuteMsg, InstantiateMsg as GaugeInstantiateMsg, PendingTokenResponse,
        QueryMsg as GaugeQueryMsg,
    },
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        ExecuteMsg as VestingExecuteMsg, InstantiateMsg as VestingInstantiateMsg, VestingAccount,
    },
};
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR},
    to_binary, Addr, StdResult, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use gauge_proxy_interface::msg::InstantiateMsg as ProxyInstantiateMsg;
use mirror_protocol::staking::{
    Cw20HookMsg as MirrorStakingHookMsg, ExecuteMsg as MirrorExecuteMsg,
    InstantiateMsg as MirrorInstantiateMsg,
};
use terra_multi_test::{next_block, App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

const OWNER: &str = "Owner";
const USER1: &str = "User1";
const USER2: &str = "User2";

#[test]
fn gauge_without_reward_proxies() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);

    let lp_cny_eur_instance = instantiate_token(&mut app, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&mut app, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let gauge_instance = instantiate_gauge(&mut app, &astro_token_instance);

    register_lp_tokens_in_gauge(
        &mut app,
        &gauge_instance,
        None,
        &[&lp_cny_eur_instance, &lp_eur_usd_instance],
    );

    // User 1
    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 9);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user1, 10);

    allow_tokens(&mut app, &lp_cny_eur_instance, USER1, &gauge_instance, 10);
    allow_tokens(&mut app, &lp_eur_usd_instance, USER1, &gauge_instance, 10);

    // An user can't deposit without sufficient lp_token balance
    let msg = GaugeExecuteMsg::Deposit {
        lp_token: (lp_cny_eur_instance).clone(),
        amount: Uint128::new(10),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 1);

    deposit_lp_tokens_to_gauge(
        &mut app,
        &gauge_instance,
        USER1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 10);

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    // An user can't withdraw if didn't deposit
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // An user can't emergency withdraw if didn't deposit
    let msg = GaugeExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.clone(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "astroport_gauge::state::UserInfo not found".to_string()
    );

    app.update_block(|bi| next_block(bi));

    // 10 per block by 5 for two pools having the same alloc points
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, &lp_cny_eur_instance, &user2, 10);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user2, 10);

    allow_tokens(&mut app, &lp_cny_eur_instance, USER2, &gauge_instance, 10);
    allow_tokens(&mut app, &lp_eur_usd_instance, USER2, &gauge_instance, 10);

    deposit_lp_tokens_to_gauge(
        &mut app,
        &gauge_instance,
        USER2,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 20);

    // 10 distributed to gauge contract after last deposit
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 10_000000);

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // new deposits can't receive already calculated rewards
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (0, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (0, None),
    );

    // change pool alloc points
    let msg = GaugeExecuteMsg::Set {
        alloc_point: Uint64::new(60),
        lp_token: lp_cny_eur_instance.clone(),
        with_update: true,
    };
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();
    let msg = GaugeExecuteMsg::Set {
        alloc_point: Uint64::new(40),
        lp_token: lp_eur_usd_instance.clone(),
        with_update: true,
    };
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (8_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (3_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already fixed rewards (5).
    // Pending tokens (3) will be redistributed to other staking users.
    let msg = GaugeExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.clone(),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (0_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (6_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // balance of the gauge should be decreased
    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    let msg = GaugeExecuteMsg::MassUpdatePools {};
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 20_000000);
    check_token_balance(&mut app, &astro_token_instance, &owner, 0_000000);

    // Owner sends orphan astro rewards
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: None,
        amount: Uint128::new(5_000000),
    };

    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 15_000000);
    check_token_balance(&mut app, &astro_token_instance, &owner, 5_000000);

    // Owner can't send astro rewards for distribution to users
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: None,
        amount: Uint128::new(0_000001),
    };

    assert_eq!(
        app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient amount of orphan rewards!"
    );

    // User2 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000);
    // Astro balance of the contract is 7 + 2 (for other pool) (5 left on emergency withdraw, 6 transfered to User2)
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 9_000000);

    // User1 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User1 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User2 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000 + 2_000000);
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 0_000000);
}

#[test]
fn gauge_with_mirror_reward_proxy() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);

    let pair_cny_eur_instance = Addr::unchecked("cny-eur pair");

    let lp_cny_eur_instance = instantiate_token(&mut app, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&mut app, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let gauge_instance = instantiate_gauge(&mut app, &astro_token_instance);

    let (mirror_token_instance, mirror_staking_instance) = instantiate_mirror_protocol(
        &mut app,
        token_code_id,
        &pair_cny_eur_instance,
        &lp_cny_eur_instance,
    );

    let proxy_code_id = store_proxy_code(&mut app);

    let proxy_to_mirror_instance = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &gauge_instance,
        &pair_cny_eur_instance,
        &lp_cny_eur_instance,
        &mirror_staking_instance,
        &mirror_token_instance,
    );

    // can't add if proxy isn't allowed
    let msg = GaugeExecuteMsg::Add {
        alloc_point: Uint64::from(100u64),
        reward_proxy: Some(proxy_to_mirror_instance.to_string()),
        lp_token: lp_cny_eur_instance.clone(),
        with_update: true,
    };
    assert_eq!(
        app.execute_contract(Addr::unchecked(OWNER), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        String::from("Reward proxy not allowed!")
    );

    let msg = GaugeExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string()],
    };
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    register_lp_tokens_in_gauge(
        &mut app,
        &gauge_instance,
        Some(&proxy_to_mirror_instance),
        &[&lp_cny_eur_instance],
    );

    register_lp_tokens_in_gauge(&mut app, &gauge_instance, None, &[&lp_eur_usd_instance]);

    // User 1
    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 9);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user1, 10);

    allow_tokens(&mut app, &lp_cny_eur_instance, USER1, &gauge_instance, 10);
    allow_tokens(&mut app, &lp_eur_usd_instance, USER1, &gauge_instance, 10);

    // An user can't deposit without sufficient lp_token balance
    let msg = GaugeExecuteMsg::Deposit {
        lp_token: (lp_cny_eur_instance).clone(),
        amount: Uint128::new(10),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 1);

    deposit_lp_tokens_to_gauge(
        &mut app,
        &gauge_instance,
        USER1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    // With the proxy the gauge contract doesn't have the deposited lp tokens
    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 0);
    // the lp tokens are in the end contract now
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 10);

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &mirror_staking_instance, 0);

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, Some(0)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    // An user can't withdraw if didn't deposit
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // An user can't emergency withdraw if didn't deposit
    let msg = GaugeExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.clone(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "astroport_gauge::state::UserInfo not found".to_string()
    );

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur_instance.to_string(), Uint128::new(50_000000))],
        })
        .unwrap(),
        amount: Uint128::new(50_000000),
    };

    mint_tokens(&mut app, &mirror_token_instance, &owner, 50_000000);
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    // 10 per block by 5 for two pools having the same alloc points
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, Some(50_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, &lp_cny_eur_instance, &user2, 10);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user2, 10);

    allow_tokens(&mut app, &lp_cny_eur_instance, USER2, &gauge_instance, 10);
    allow_tokens(&mut app, &lp_eur_usd_instance, USER2, &gauge_instance, 10);

    deposit_lp_tokens_to_gauge(
        &mut app,
        &gauge_instance,
        USER2,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 20);

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd_instance, &mirror_staking_instance, 0);

    // 10 distributed to gauge contract after last deposit
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 10_000000);
    // 5 distrubuted to proxy contract after last deposit
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        50_000000,
    );

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, Some(50_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // new deposits can't receive already calculated rewards
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (0, Some(0)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (0, None),
    );

    // change pool alloc points
    let msg = GaugeExecuteMsg::Set {
        alloc_point: Uint64::new(60),
        lp_token: lp_cny_eur_instance.clone(),
        with_update: true,
    };
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();
    let msg = GaugeExecuteMsg::Set {
        alloc_point: Uint64::new(40),
        lp_token: lp_eur_usd_instance.clone(),
        with_update: true,
    };
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur_instance.to_string(), Uint128::new(60_000000))],
        })
        .unwrap(),
        amount: Uint128::new(60_000000),
    };

    mint_tokens(&mut app, &mirror_token_instance, &owner, 60_000000);
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (8_000000, Some(80_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (3_000000, Some(30_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already fixed rewards (5).
    // Pending tokens (3) will be redistributed to other staking users.
    let msg = GaugeExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.clone(),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER1,
        (0_000000, Some(0_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_cny_eur_instance,
        USER2,
        (6_000000, Some(60_000000)),
    );
    check_pending_rewards(
        &mut app,
        &gauge_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // balance of the end contract should be decreased
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    let msg = GaugeExecuteMsg::MassUpdatePools {};
    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 20_000000);
    check_token_balance(&mut app, &astro_token_instance, &owner, 0_000000);

    // Owner sends orphan astro rewards
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: None,
        amount: Uint128::new(5_000000),
    };

    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 15_000000);
    check_token_balance(&mut app, &astro_token_instance, &owner, 5_000000);

    // Owner can't send astro rewards for distribution to users
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: None,
        amount: Uint128::new(0_000001),
    };

    assert_eq!(
        app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient amount of orphan rewards!"
    );

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        110_000000,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 0_000000);

    // Owner sends orphan proxy rewards
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: Some(lp_cny_eur_instance.to_string()),
        amount: Uint128::new(50_000000),
    };

    app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        60_000000,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 50_000000);

    // Owner can't send proxy rewards for distribution to users
    let msg = GaugeExecuteMsg::SendOrphanReward {
        recipient: owner.to_string(),
        lp_token: Some(lp_cny_eur_instance.to_string()),
        amount: Uint128::new(0_000001),
    };

    assert_eq!(
        app.execute_contract(owner.clone(), gauge_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient amount of orphan rewards!"
    );

    // User2 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.clone(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur_instance, &gauge_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    // Astro balance of the contract is 7 + 2 (for other pool) (5 left on emergency withdraw, 6 transfered to User2)
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 9_000000);
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );

    // User1 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User1 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User2 withdraw and get rewards
    let msg = GaugeExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.clone(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), gauge_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &gauge_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000 + 2_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    check_token_balance(&mut app, &astro_token_instance, &gauge_instance, 0_000000);
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );
}

fn mock_app() -> App {
    let api = MockApi::default();
    let env = mock_env();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let terra_mock_querier = TerraMockQuerier::new(MockQuerier::new(&[]));

    App::new(api, env.block, bank, storage, terra_mock_querier)
}

fn store_token_code(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new(
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
            minter: String::from(OWNER),
            cap: cap.map(|v| Uint128::from(v)),
        }),
        init_hook: None,
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

fn instantiate_gauge(mut app: &mut App, astro_token_instance: &Addr) -> Addr {
    // Vesting
    let vesting_contract = Box::new(ContractWrapper::new(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));

    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = VestingInstantiateMsg {
        genesis_time: app.block_info().time,
        owner: Addr::unchecked(OWNER),
        token_addr: astro_token_instance.clone(),
    };

    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            Addr::unchecked(OWNER),
            &init_msg,
            &[],
            "Vesting",
            None,
        )
        .unwrap();

    mint_tokens(
        &mut app,
        &astro_token_instance,
        &vesting_instance,
        1_000_000_000_000000,
    );

    // Gauge
    let gauge_contract = Box::new(
        ContractWrapper::new(
            astroport_gauge::contract::execute,
            astroport_gauge::contract::instantiate,
            astroport_gauge::contract::query,
        )
        .with_reply(astroport_gauge::contract::reply),
    );

    let gauge_code_id = app.store_code(gauge_contract);

    let init_msg = GaugeInstantiateMsg {
        allowed_reward_proxies: vec![],
        start_block: Uint64::from(app.block_info().height),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::from(10_000000u128),
        vesting_contract: vesting_instance.to_string(),
    };

    let gauge_instance = app
        .instantiate_contract(
            gauge_code_id,
            Addr::unchecked(OWNER),
            &init_msg,
            &[],
            "Guage",
            None,
        )
        .unwrap();

    // vesting to gauge:

    let current_block = app.block_info();

    let msg = VestingExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: gauge_instance.clone(),
            schedules: vec![(
                current_block.time,
                current_block.time.plus_seconds(365 * 24 * 60 * 60),
                Uint128::new(63072000_000000),
            )],
        }],
    };

    app.execute_contract(Addr::unchecked(OWNER), vesting_instance, &msg, &[])
        .unwrap();

    gauge_instance
}

fn instantiate_mirror_protocol(
    app: &mut App,
    token_code_id: u64,
    asset_token: &Addr,
    staking_token: &Addr,
) -> (Addr, Addr) {
    let mirror_token_instance = instantiate_token(app, token_code_id, "MIR", None);

    // Mirror staking
    let mirror_staking_contract = Box::new(ContractWrapper::new(
        mirror_staking::contract::execute,
        mirror_staking::contract::instantiate,
        mirror_staking::contract::query,
    ));

    let mirror_staking_code_id = app.store_code(mirror_staking_contract);

    let init_msg = MirrorInstantiateMsg {
        base_denom: String::from("uusd"),
        mint_contract: String::from(MOCK_CONTRACT_ADDR),
        mirror_token: mirror_token_instance.to_string(),
        oracle_contract: String::from(MOCK_CONTRACT_ADDR),
        owner: String::from(OWNER),
        premium_min_update_interval: 0,
        short_reward_contract: String::from(MOCK_CONTRACT_ADDR),
        terraswap_factory: String::from(MOCK_CONTRACT_ADDR),
    };

    let mirror_staking_instance = app
        .instantiate_contract(
            mirror_staking_code_id,
            Addr::unchecked(OWNER),
            &init_msg,
            &[],
            "Mirror staking",
            None,
        )
        .unwrap();

    let msg = MirrorExecuteMsg::RegisterAsset {
        asset_token: asset_token.to_string(),
        staking_token: staking_token.to_string(),
    };

    app.execute_contract(
        Addr::unchecked(OWNER),
        mirror_staking_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    (mirror_token_instance, mirror_staking_instance)
}

fn store_proxy_code(app: &mut App) -> u64 {
    let gauge_proxy_to_mirror_contract = Box::new(ContractWrapper::new(
        gauge_proxy_to_mirror::contract::execute,
        gauge_proxy_to_mirror::contract::instantiate,
        gauge_proxy_to_mirror::contract::query,
    ));

    app.store_code(gauge_proxy_to_mirror_contract)
}

fn instantiate_proxy(
    app: &mut App,
    proxy_code: u64,
    gauge_instance: &Addr,
    pair: &Addr,
    lp_token: &Addr,
    mirror_staking_instance: &Addr,
    mirror_token_instance: &Addr,
) -> Addr {
    let init_msg = ProxyInstantiateMsg {
        gauge_contract_addr: gauge_instance.to_string(),
        pair_addr: pair.to_string(),
        lp_token_addr: lp_token.to_string(),
        reward_contract_addr: mirror_staking_instance.to_string(),
        reward_token_addr: mirror_token_instance.to_string(),
    };

    app.instantiate_contract(
        proxy_code,
        Addr::unchecked(OWNER),
        &init_msg,
        &[],
        String::from("Proxy"),
        None,
    )
    .unwrap()
}

fn register_lp_tokens_in_gauge(
    app: &mut App,
    gauge_instance: &Addr,
    reward_proxy: Option<&Addr>,
    lp_tokens: &[&Addr],
) {
    for lp in lp_tokens {
        let msg = GaugeExecuteMsg::Add {
            alloc_point: Uint64::from(100u64),
            reward_proxy: reward_proxy.map(|v| v.to_string()),
            lp_token: (*lp).clone(),
            with_update: true,
        };
        app.execute_contract(Addr::unchecked(OWNER), gauge_instance.clone(), &msg, &[])
            .unwrap();
    }
}

fn mint_tokens(app: &mut App, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER), token.to_owned(), &msg, &[])
        .unwrap();
}

fn allow_tokens(app: &mut App, token: &Addr, owner: &str, spender: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        expires: None,
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(owner), token.to_owned(), &msg, &[])
        .unwrap();
}

fn deposit_lp_tokens_to_gauge(
    app: &mut App,
    gauge_instance: &Addr,
    depositor: &str,
    lp_tokens: &[(&Addr, u128)],
) {
    for (token, amount) in lp_tokens {
        let msg = GaugeExecuteMsg::Deposit {
            lp_token: (*token).clone(),
            amount: Uint128::from(amount.to_owned()),
        };

        app.execute_contract(
            Addr::unchecked(depositor),
            gauge_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    }
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn check_pending_rewards(
    app: &mut App,
    gauge_instance: &Addr,
    token: &Addr,
    depositor: &str,
    expected: (u128, Option<u128>),
) {
    let msg = GaugeQueryMsg::PendingToken {
        lp_token: token.to_owned(),
        user: Addr::unchecked(depositor),
    };

    let res: PendingTokenResponse = app
        .wrap()
        .query_wasm_smart(gauge_instance.to_owned(), &msg)
        .unwrap();
    assert_eq!(
        (res.pending, res.pending_on_proxy),
        (
            Uint128::from(expected.0),
            expected.1.map(|v| Uint128::from(v))
        )
    );
}
