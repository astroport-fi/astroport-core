use std::convert::TryInto;

use astroport::generator::{ExecuteMsg, QueryMsg};
use astroport::{
    generator::{
        ConfigResponse, Cw20HookMsg as GeneratorHookMsg, ExecuteMsg as GeneratorExecuteMsg,
        InstantiateMsg as GeneratorInstantiateMsg, PendingTokenResponse,
        QueryMsg as GeneratorQueryMsg,
    },
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg as VestingHookMsg, InstantiateMsg as VestingInstantiateMsg, VestingAccount,
        VestingSchedule, VestingSchedulePoint,
    },
};
use cosmwasm_std::Coin;
use cosmwasm_std::{
    to_binary, Addr, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account};

const SECONDS_PER_BLOCK: u64 = 5;

#[test]
fn disabling_pool() {
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

    let lp_cny_eur_instance = instantiate_token(&wasm, owner, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&wasm, owner, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    register_lp_tokens_in_generator(
        &wasm,
        owner,
        &generator_instance,
        None,
        &[&lp_cny_eur_instance, &lp_eur_usd_instance],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&wasm, owner, &lp_cny_eur_instance, &Addr::unchecked(user1.address()), 10);
    mint_tokens(&wasm, owner, &lp_eur_usd_instance, &Addr::unchecked(user1.address()), 10);

    deposit_lp_tokens_to_generator(
        &wasm,
        &generator_instance,
        user1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 10);
    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 10);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (0, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (0, None),
    );

    app.increase_time(SECONDS_PER_BLOCK);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (5000000, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (5000000, None),
    );

    // setting the allocation point to zero for pool
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(0),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };

    wasm.execute(generator_instance.as_str(), &msg, &[], owner).unwrap();

    // setting the allocation point to zero for pool
    let msg_eur_usd = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(0),
        lp_token: lp_eur_usd_instance.to_string(),
        has_asset_rewards: false,
    };
    wasm.execute(generator_instance.as_str(), &msg_eur_usd, &[], owner).unwrap();

    app.increase_time(SECONDS_PER_BLOCK);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (5000000, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (5000000, None),
    );

    app.increase_time(SECONDS_PER_BLOCK);

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(10),
    };

    wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap();

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(10),
    };

    wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap();

    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 0);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (0, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (0, None),
    );

    app.increase_time(SECONDS_PER_BLOCK);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (0, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (0, None),
    );
}

#[test]
fn set_tokens_per_block() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        1
    ).unwrap();
    let owner = &accs[0];

    let token_code_id = store_token_code(&wasm, owner);
    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.tokens_per_block, Uint128::new(10_000000));

    // setting new value of tokens per block
    let tokens_per_block = Uint128::new(100);

    let msg = GeneratorExecuteMsg::SetTokensPerBlock {
        amount: tokens_per_block,
    };
    wasm.execute(
        generator_instance.as_str(), 
        &msg, 
        &[], 
    owner).unwrap();

    let msg = GeneratorQueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();
    assert_eq!(res.tokens_per_block, tokens_per_block);
}

#[test]
fn update_config() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        1
    ).unwrap();

    let owner = &accs[0];

    let token_code_id = store_token_code(&wasm, owner);
    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.owner.to_string(), owner.address());
    assert_eq!(res.astro_token.to_string(), "contract #0");
    assert_eq!(res.vesting_contract.to_string(), "contract #1");

    let new_vesting = Addr::unchecked("new_vesting");

    let msg = ExecuteMsg::UpdateConfig {
        vesting_contract: Some(new_vesting.to_string()),
    };

    // Assert cannot update with improper owner
    let unauthorized = &app.init_account(&[]).unwrap();
    let e = wasm.execute(
        generator_instance.as_str(), 
        &msg, 
        &[], 
        unauthorized
    ).unwrap_err();
    assert_eq!(e.to_string(), "Unauthorized");

    wasm.execute(
        generator_instance.as_str(), 
        &msg, 
        &[], 
    owner
    ).unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.vesting_contract, new_vesting);
}

#[test]
fn update_owner() {
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
    let new_owner = &accs[1];

    let token_code_id = store_token_code(&wasm, owner);
    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    // new owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.address(),
        expires_in: 100, // seconds
    };

    // unauthorized check
    let unauthorized = &app.init_account(&[]).unwrap();
    let e = wasm.execute(
        generator_instance.as_str(), 
        &msg, 
        &[], 
        unauthorized
    ).unwrap_err();
    assert_eq!(e.to_string(), "Generic error: Unauthorized");

    // claim before proposal
    let err = wasm.execute(
        generator_instance.as_str(), 
        &ExecuteMsg::ClaimOwnership {}, 
        &[], 
    new_owner
    ).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // propose new owner
    wasm.execute(
        generator_instance.as_str(), 
        &msg, 
        &[], 
    owner
    ).unwrap();

    // claim from invalid addr
    let invalid_addr = &app.init_account(&[]).unwrap();
    let err = wasm.execute(
        generator_instance.as_str(), 
        &ExecuteMsg::ClaimOwnership {}, 
        &[], 
    invalid_addr
    ).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // claim ownership
    wasm.execute(
        generator_instance.as_str(), 
        &ExecuteMsg::ClaimOwnership {}, 
        &[], 
    new_owner
    ).unwrap();

    // let's query the state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.owner.to_string(), new_owner.address())
}

#[test]
fn send_from_unregistered_lp() {
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

    let lp_eur_usdt_instance = instantiate_token(&wasm, owner, token_code_id, "EUR-USDT", None);

    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    // Mint tokens, so user can deposit
    mint_tokens(&wasm, owner, &lp_eur_usdt_instance, &Addr::unchecked(user1.address()), 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    let resp = wasm.execute(
        lp_eur_usdt_instance.as_str(), 
        &msg, 
        &[], 
        user1
    ).unwrap_err();
    assert_eq!(resp.to_string(), "Unauthorized");

    // Register lp token
    register_lp_tokens_in_generator(
        &wasm,
        owner,
        &generator_instance,
        None,
        &[&lp_eur_usdt_instance],
    );

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    wasm.execute(
        lp_eur_usdt_instance.as_str(), 
        &msg, 
        &[], 
        user1
    ).unwrap();
}

#[test]
fn generator_without_reward_proxies() {
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

    let lp_cny_eur_instance = instantiate_token(&wasm, owner, token_code_id, "CNY-EUR", None);
    let lp_eur_usd_instance = instantiate_token(&wasm, owner, token_code_id, "EUR-USD", None);

    let astro_token_instance =
        instantiate_token(&wasm, owner, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(&app, owner, &astro_token_instance);

    register_lp_tokens_in_generator(
        &wasm,
        owner, 
        &generator_instance,
        None,
        &[&lp_cny_eur_instance, &lp_eur_usd_instance],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&wasm, owner, &lp_cny_eur_instance, &Addr::unchecked(user1.address()), 9);
    mint_tokens(&wasm, owner, &lp_eur_usd_instance, &Addr::unchecked(user1.address()), 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    assert_eq!(
        wasm.execute(lp_cny_eur_instance.as_str(), &msg, &[], user1).unwrap_err().to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&wasm, owner,&lp_cny_eur_instance, &Addr::unchecked(user1.address()), 1);

    deposit_lp_tokens_to_generator(
        &wasm,
        &generator_instance,
        user1,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 10);
    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 10);

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (0, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (0, None),
    );

    // User can't withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        wasm.execute(generator_instance.as_str(), &msg, &[], user2).unwrap_err().to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // User can't emergency withdraw if didn't deposit
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    assert_eq!(
        wasm.execute(generator_instance.as_str(), &msg, &[], user2).unwrap_err().to_string(),
        "astroport_generator::state::UserInfo not found".to_string()
    );

    app.increase_time(SECONDS_PER_BLOCK);

    // 10 per block by 5 for two pools having the same alloc points
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (5_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (5_000000, None),
    );

    // User 2
    mint_tokens(&wasm, owner, &lp_cny_eur_instance, &Addr::unchecked(user2.address()), 10);
    mint_tokens(&wasm, owner, &lp_eur_usd_instance, &Addr::unchecked(user2.address()), 10);

    deposit_lp_tokens_to_generator(
        &wasm,
        &generator_instance,
        user2,
        &[(&lp_cny_eur_instance, 10), (&lp_eur_usd_instance, 10)],
    );

    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 20);
    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 20);

    // 10 distributed to depositors after last deposit

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (5_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (5_000000, None),
    );

    // new deposits can't receive already calculated rewards
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user2.address(),
        (0, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user2.address(),
        (0, None),
    );

    // change pool alloc points
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(60),
        lp_token: lp_cny_eur_instance.to_string(),
        has_asset_rewards: false,
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], owner).unwrap();
    let msg = GeneratorExecuteMsg::Set {
        alloc_point: Uint64::new(40),
        lp_token: lp_eur_usd_instance.to_string(),
        has_asset_rewards: false,
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], owner).unwrap();

    app.increase_time(SECONDS_PER_BLOCK);

    // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (8_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (7_000000, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user2.address(),
        (3_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user2.address(),
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already fixed rewards (5).
    // Pending tokens (3) will be redistributed to other staking users.
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur_instance.to_string(),
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap();

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user1.address(),
        (0_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user1.address(),
        (7_000000, None),
    );

    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_cny_eur_instance,
        &user2.address(),
        (6_000000, None),
    );
    check_pending_rewards(
        &wasm,
        &generator_instance,
        &lp_eur_usd_instance,
        &user2.address(),
        (2_000000, None),
    );

    // balance of the generator should be decreased
    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap_err().to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    let msg = GeneratorExecuteMsg::MassUpdatePools {};
    wasm.execute(generator_instance.as_str(), &msg, &[], owner).unwrap();

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur_instance.to_string(),
        amount: Uint128::new(10),
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], user2).unwrap();

    check_token_balance(&wasm, &lp_cny_eur_instance, &generator_instance, 0);
    check_token_balance(&wasm, &lp_cny_eur_instance, &Addr::unchecked(user1.address()), 10);
    check_token_balance(&wasm, &lp_cny_eur_instance, &Addr::unchecked(user2.address()), 10);

    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 0);
    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user2.address()), 6_000000);
    // Distributed Astro are 7 + 2 (for other pool) (5 left on emergency withdraw, 6 transfered to User2)

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap();

    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 15);
    check_token_balance(&wasm, &lp_eur_usd_instance, &Addr::unchecked(user1.address()), 5);

    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 7_000000);

    // User1 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(5),
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], user1).unwrap();

    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 10);
    check_token_balance(&wasm, &lp_eur_usd_instance, &Addr::unchecked(user1.address()), 10);
    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 7_000000);

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd_instance.to_string(),
        amount: Uint128::new(10),
    };
    wasm.execute(generator_instance.as_str(), &msg, &[], user2).unwrap();

    check_token_balance(&wasm, &lp_eur_usd_instance, &generator_instance, 0);
    check_token_balance(&wasm, &lp_eur_usd_instance, &Addr::unchecked(user1.address()), 10);
    check_token_balance(&wasm, &lp_eur_usd_instance, &Addr::unchecked(user2.address()), 10);

    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user1.address()), 7_000000);
    check_token_balance(&wasm, &astro_token_instance, &Addr::unchecked(user2.address()), 6_000000 + 2_000000);
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

fn instantiate_generator(app: &TerraTestApp, owner: &SigningAccount, astro_token_instance: &Addr) -> Addr {
    let wasm = Wasm::new(app);

    // Vesting
    let vesting_contract = std::fs::read("../../../../artifacts/astroport_staking.wasm").unwrap();
    let vesting_code_id = wasm.store_code(&vesting_contract, None, owner).unwrap().data.code_id;

    let init_msg = VestingInstantiateMsg {
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

    mint_tokens(
        &wasm,
        owner,
        &astro_token_instance,
        &Addr::unchecked(owner.address()),
        1_000_000_000_000000,
    );

    // Generator
    let generator_contract = std::fs::read("../../../../artifacts/astroport_generator.wasm").unwrap();
    let generator_code_id = wasm.store_code(&generator_contract, None, owner).unwrap().data.code_id;

    let init_msg = GeneratorInstantiateMsg {
        owner: owner.address(),
        allowed_reward_proxies: vec![],
        start_block: Uint64::new(TryInto::<u64>::try_into(app.get_block_height()).unwrap()),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::new(10_000000),
        vesting_contract: vesting_instance.clone().data.address,
    };

    let generator_instance = wasm.instantiate(
        generator_code_id, 
        &init_msg, 
        Some(&owner.address()), 
        Some("Guage"),
        &[], 
        owner
    ).unwrap();

    // vesting to generator:
    let amount = Uint128::new(63072000_000000);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.clone().data.address,
        msg: to_binary(&VestingHookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: generator_instance.clone().data.address,
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: TryInto::<u64>::try_into(app.get_block_time_seconds()).unwrap(),
                        amount,
                    },
                    end_point: None,
                }],
            }],
        })
        .unwrap(),
        amount,
    };

    wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();

    Addr::unchecked(generator_instance.clone().data.address)
}

fn register_lp_tokens_in_generator(
    wasm: &Wasm<TerraTestApp>, 
    owner: &SigningAccount,
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
        wasm.execute(
            generator_instance.as_str(), 
            &msg, 
            &[], 
            owner
        ).unwrap();
    }
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

fn deposit_lp_tokens_to_generator(
    wasm: &Wasm<TerraTestApp>, 
    generator_instance: &Addr,
    depositor: &SigningAccount,
    lp_tokens: &[(&Addr, u128)],
) {
    for (token, amount) in lp_tokens {
        let msg = Cw20ExecuteMsg::Send {
            contract: generator_instance.to_string(),
            msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
            amount: Uint128::from(amount.to_owned()),
        };

        wasm.execute(
            (*token).as_str(), 
            &msg, 
            &[], 
            depositor
        ).unwrap();
    }
}

fn check_token_balance(wasm: &Wasm<TerraTestApp>, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: BalanceResponse = wasm.query(token.as_str(), &msg).unwrap();
    assert_eq!(res.balance, Uint128::from(expected));
}

fn check_pending_rewards(
    wasm: &Wasm<TerraTestApp>,
    generator_instance: &Addr,
    token: &Addr,
    depositor: &str,
    expected: (u128, Option<u128>),
) {
    let msg = GeneratorQueryMsg::PendingToken {
        lp_token: token.to_string(),
        user: String::from(depositor),
    };

    let res: PendingTokenResponse = wasm
        .query(generator_instance.as_str(), &msg)
        .unwrap();
    assert_eq!(
        (res.pending, res.pending_on_proxy),
        (
            Uint128::from(expected.0),
            expected.1.map(|v| Uint128::from(v))
        )
    );
}
