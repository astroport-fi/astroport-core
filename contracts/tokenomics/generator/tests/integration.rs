use astroport::generator::{ExecuteMsg, QueryMsg};
use astroport::{
    generator::{
        ConfigResponse, Cw20HookMsg as GeneratorHookMsg, ExecuteMsg as GeneratorExecuteMsg,
        InstantiateMsg as GeneratorInstantiateMsg, PendingTokenResponse,
        QueryMsg as GeneratorQueryMsg,
    },
    generator_proxy::InstantiateMsg as ProxyInstantiateMsg,
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg as VestingHookMsg, InstantiateMsg as VestingInstantiateMsg, VestingAccount,
        VestingSchedule, VestingSchedulePoint,
    },
};
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR},
    to_binary, Addr, StdResult, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use mirror_protocol::staking::{
    Cw20HookMsg as MirrorStakingHookMsg, ExecuteMsg as MirrorExecuteMsg,
    InstantiateMsg as MirrorInstantiateMsg,
};
use terra_multi_test::{
    next_block, AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";

#[test]
fn disabling_pool() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);

    let token_code_id = store_token_code(&mut app);

    let lp_cny_eur_instance = instantiate_token(&mut app, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&mut app, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        None,
        &[&lp_cny_eur_instance, &lp_eur_usd_instance],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 10);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user1, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 10);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    app.update_block(|bi| next_block(bi));

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5000000, None),
    );

    // setting the allocation point to zero for pool
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(0),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // setting the allocation point to zero for pool
    let msg_eur_usd = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(0),
        lp_token: lp_eur_usd_instance.to_string(),
        has_asset_rewards: false,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg_eur_usd, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5000000, None),
    );

    app.update_block(|bi| next_block(bi));

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 0);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    app.update_block(|bi| next_block(bi));

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );
}

#[test]
fn set_tokens_per_block() {
    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.tokens_per_block, Uint128::new(10_000000));

    // setting new value of tokens per block
    let tokens_per_block = Uint128::new(100);

    let msg = GeneratorExecuteMsg::SetTokensPerBlock {
        amount: tokens_per_block,
    };
    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = GeneratorQueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();
    assert_eq!(res.tokens_per_block, tokens_per_block);
}

#[test]
fn update_config() {
    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.owner, OWNER);
    assert_eq!(res.astro_token.to_string(), "contract #0");
    assert_eq!(res.vesting_contract.to_string(), "contract #1");

    let new_vesting = Addr::unchecked("new_vesting");

    let msg = ExecuteMsg::UpdateConfig {
        vesting_contract: Some(new_vesting.to_string()),
    };

    // Assert cannot update with improper owner
    let e = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            generator_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert_eq!(e.to_string(), "Unauthorized");

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.vesting_contract, new_vesting);
}

#[test]
fn update_owner() {
    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    let new_owner = String::from("new_owner");

    // new owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // unauthorized check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            generator_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            generator_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // propose new owner
    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            generator_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        generator_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // let's query the state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.owner.to_string(), new_owner)
}

#[test]
fn send_from_unregistered_lp() {
    let mut app = mock_app();

    let user1 = Addr::unchecked(USER1);

    let token_code_id = store_token_code(&mut app);

    let lp_eur_usdt_instance = instantiate_token(&mut app, token_code_id, "EUR-USDT", None);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, &lp_eur_usdt_instance, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    let resp = app
        .execute_contract(user1.clone(), lp_eur_usdt_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(resp.to_string(), "Unauthorized");

    // Register lp token
    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        None,
        &[&lp_eur_usdt_instance],
    );

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), (lp_eur_usdt_instance).clone(), &msg, &[])
        .unwrap();
}

#[test]
fn generator_without_reward_proxies() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);

    let lp_cny_eur_instance = instantiate_token(&mut app, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&mut app, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        None,
        &[&lp_cny_eur_instance, &lp_eur_usd_instance],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 9);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    assert_eq!(
        app.execute_contract(user1.clone(), (lp_cny_eur_instance).clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 1);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 10);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    // User can't withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // User can't emergency withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "astroport_generator::state::UserInfo not found".to_string()
    );

    app.update_block(|bi| next_block(bi));

    // 10 per block by 5 for two pools having the same alloc points
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, &lp_cny_eur_instance, &user2, 10);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user2, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER2,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 20);

    // 10 distributed to depositors after last deposit

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // new deposits can't receive already calculated rewards
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (0, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (0, None),
    );

    // change pool alloc points
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(60),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(40),
        lp_token: lp_eur_usd_instance.to_string(),
        has_asset_rewards: false,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (8_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (3_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already fixed rewards (5).
    // Pending tokens (3) will be redistributed to other staking users.
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (6_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // balance of the generator should be decreased
    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    let msg = GeneratorExecuteMsg::MassUpdatePools {};
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000);
    // Distributed Astro are 7 + 2 (for other pool) (5 left on emergency withdraw, 6 transfered to User2)

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000 + 2_000000);
}

#[test]
fn generator_with_mirror_reward_proxy() {
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

    let generator_instance = instantiate_generator(&mut app, &astro_token_instance);

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
        &generator_instance,
        &pair_cny_eur_instance,
        &lp_cny_eur_instance,
        &mirror_staking_instance,
        &mirror_token_instance,
    );

    // can't add if proxy isn't allowed
    let msg = GeneratorExecuteMsg::Add {
        alloc_point: Uint64::from(100u64),
        reward_proxy: Some(proxy_to_mirror_instance.to_string()),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };
    assert_eq!(
        app.execute_contract(
            Addr::unchecked(OWNER),
            generator_instance.clone(),
            &msg,
            &[]
        )
        .unwrap_err()
        .to_string(),
        String::from("Reward proxy not allowed!")
    );

    let msg = GeneratorExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string()],
    };
    assert_eq!(
        app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        String::from("Unauthorized")
    );

    let msg = GeneratorExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string()],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        Some(&proxy_to_mirror_instance),
        &[&lp_cny_eur_instance],
    );

    register_lp_tokens_in_generator(&mut app, &generator_instance, None, &[&lp_eur_usd_instance]);

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 9);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    assert_eq!(
        app.execute_contract(user1.clone(), (lp_cny_eur_instance).clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, &lp_cny_eur_instance, &user1, 1);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    // With the proxy the generator contract doesn't have the deposited lp tokens
    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 0);
    // the lp tokens are in the end contract now
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 10);

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &mirror_staking_instance, 0);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0, Some(0)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (0, None),
    );

    // User can't withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // User can't emergency withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "astroport_generator::state::UserInfo not found".to_string()
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
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, Some(50_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, &lp_cny_eur_instance, &user2, 10);
    mint_tokens(&mut app, &lp_eur_usd_instance, &user2, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER2,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 20);

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd_instance, &mirror_staking_instance, 0);

    // 10 distributed to depositors after last deposit

    // 5 distrubuted to proxy contract after last deposit
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        50_000000,
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (5_000000, Some(50_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (5_000000, None),
    );

    // new deposits can't receive already calculated rewards
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (0, Some(0)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (0, None),
    );

    // change pool alloc points
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(60),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(40),
        lp_token: lp_eur_usd_instance.to_string(),
        has_asset_rewards: false,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
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
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (8_000000, Some(80_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (3_000000, Some(30_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already fixed rewards (5).
    // Pending tokens (3) will be redistributed to other staking users.
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER1,
        (0_000000, Some(0_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur_instance,
        USER2,
        (6_000000, Some(60_000000)),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd_instance,
        USER2,
        (2_000000, None),
    );

    // balance of the end contract should be decreased
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    let msg = GeneratorExecuteMsg::MassUpdatePools {};
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        110_000000,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 0_000000);

    // Check if there are orphan proxy rewards
    let msg = GeneratorQueryMsg::OrphanProxyRewards {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    let orphan_rewards: Uint128 = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();
    assert_eq!(orphan_rewards, Uint128::new(50_000000));

    // Owner sends orphan proxy rewards
    let msg = GeneratorExecuteMsg::SendOrphanProxyReward {
        recipient: owner.to_string(),
        lp_token: lp_cny_eur_instance.to_string(),
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        60_000000,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 50_000000);

    // Owner can't send proxy rewards for distribution to users
    let msg = GeneratorExecuteMsg::SendOrphanProxyReward {
        recipient: owner.to_string(),
        lp_token: lp_cny_eur_instance.to_string(),
    };

    assert_eq!(
        app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient amount of orphan rewards!"
    );

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &mirror_staking_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    // Distributed Astro are 7 + 2 (for other pool) (5 left on emergency withdraw, 6 transfered to User2)
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd_instance, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd_instance, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 6_000000 + 2_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );
}

fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

fn store_token_code(app: &mut TerraApp) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn instantiate_token(
    app: &mut TerraApp,
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
            minter: String::from(OWNER),
            cap: cap.map(|v| Uint128::from(v)),
        }),
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

fn instantiate_generator(mut app: &mut TerraApp, astro_token_instance: &Addr) -> Addr {
    // Vesting
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let owner = Addr::unchecked(OWNER);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = VestingInstantiateMsg {
        owner: owner.to_string(),
        token_addr: astro_token_instance.to_string(),
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

    mint_tokens(
        &mut app,
        &astro_token_instance,
        &owner,
        1_000_000_000_000000,
    );

    // Generator
    let generator_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_generator::contract::execute,
            astroport_generator::contract::instantiate,
            astroport_generator::contract::query,
        )
        .with_reply_empty(astroport_generator::contract::reply),
    );

    let generator_code_id = app.store_code(generator_contract);

    let init_msg = GeneratorInstantiateMsg {
        owner: owner.to_string(),
        allowed_reward_proxies: vec![],
        start_block: Uint64::from(app.block_info().height),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::new(10_000000),
        vesting_contract: vesting_instance.to_string(),
    };

    let generator_instance = app
        .instantiate_contract(
            generator_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Guage",
            None,
        )
        .unwrap();

    // vesting to generator:

    let current_block = app.block_info();

    let amount = Uint128::new(63072000_000000);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&VestingHookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: generator_instance.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_block.time.seconds(),
                        amount,
                    },
                    end_point: None,
                }],
            }],
        })
        .unwrap(),
        amount,
    };

    app.execute_contract(owner, astro_token_instance.clone(), &msg, &[])
        .unwrap();

    generator_instance
}

fn instantiate_mirror_protocol(
    app: &mut TerraApp,
    token_code_id: u64,
    asset_token: &Addr,
    staking_token: &Addr,
) -> (Addr, Addr) {
    let mirror_token_instance = instantiate_token(app, token_code_id, "MIR", None);

    // Mirror staking
    let mirror_staking_contract = Box::new(ContractWrapper::new_with_empty(
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

fn store_proxy_code(app: &mut TerraApp) -> u64 {
    let generator_proxy_to_mirror_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_generator_proxy_to_mirror::contract::execute,
        astroport_generator_proxy_to_mirror::contract::instantiate,
        astroport_generator_proxy_to_mirror::contract::query,
    ));

    app.store_code(generator_proxy_to_mirror_contract)
}

fn instantiate_proxy(
    app: &mut TerraApp,
    proxy_code: u64,
    generator_instance: &Addr,
    pair: &Addr,
    lp_token: &Addr,
    mirror_staking_instance: &Addr,
    mirror_token_instance: &Addr,
) -> Addr {
    let init_msg = ProxyInstantiateMsg {
        generator_contract_addr: generator_instance.to_string(),
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

fn register_lp_tokens_in_generator(
    app: &mut TerraApp,
    generator_instance: &Addr,
    reward_proxy: Option<&Addr>,
    lp_tokens: &[&Addr],
) {
    for lp in lp_tokens {
        let msg = GeneratorExecuteMsg::Add {
            alloc_point: Uint64::from(100u64),
            reward_proxy: reward_proxy.map(|v| v.to_string()),
            lp_token: (*lp).to_string(),
            has_asset_rewards: false,
        };
        app.execute_contract(
            Addr::unchecked(OWNER),
            generator_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    }
}

fn mint_tokens(app: &mut TerraApp, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER), token.to_owned(), &msg, &[])
        .unwrap();
}

fn deposit_lp_tokens_to_generator(
    app: &mut TerraApp,
    generator_instance: &Addr,
    depositor: &str,
    lp_tokens: &[(&Addr, u128)],
) {
    for (token, amount) in lp_tokens {
        let msg = Cw20ExecuteMsg::Send {
            contract: generator_instance.to_string(),
            msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
            amount: Uint128::from(amount.to_owned()),
        };

        app.execute_contract(Addr::unchecked(depositor), (*token).clone(), &msg, &[])
            .unwrap();
    }
}

fn check_token_balance(app: &mut TerraApp, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn check_pending_rewards(
    app: &mut TerraApp,
    generator_instance: &Addr,
    token: &Addr,
    depositor: &str,
    expected: (u128, Option<u128>),
) {
    let msg = GeneratorQueryMsg::PendingToken {
        lp_token: token.to_string(),
        user: String::from(depositor),
    };

    let res: PendingTokenResponse = app
        .wrap()
        .query_wasm_smart(generator_instance.to_owned(), &msg)
        .unwrap();
    assert_eq!(
        (res.pending, res.pending_on_proxy),
        (
            Uint128::from(expected.0),
            expected.1.map(|v| Uint128::from(v))
        )
    );
}
