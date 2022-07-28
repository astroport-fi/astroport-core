use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
use astroport::generator::{ExecuteMsg, MigrateMsg, QueryMsg, StakerResponse};
use astroport_governance::utils::WEEK;

use astroport::{
    factory::{
        ConfigResponse as FactoryConfigResponse, ExecuteMsg as FactoryExecuteMsg,
        InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType, QueryMsg as FactoryQueryMsg,
    },
    generator::{
        Cw20HookMsg as GeneratorHookMsg, ExecuteMsg as GeneratorExecuteMsg,
        InstantiateMsg as GeneratorInstantiateMsg, PendingTokenResponse, PoolInfoResponse,
        QueryMsg as GeneratorQueryMsg,
    },
    //generator_proxy::InstantiateMsg as ProxyInstantiateMsg,
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg as VestingHookMsg, InstantiateMsg as VestingInstantiateMsg, VestingAccount,
        VestingSchedule, VestingSchedulePoint,
    },
};

use astroport::pair::StablePoolParams;
use astroport_generator::state::Config;
use astroport_package_generator_v110::generator as astroport_110;
use cosmwasm_std::{to_binary, Addr, Binary, StdResult, Uint128, Uint64};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{next_block, App, ContractWrapper, Executor};

use crate::test_utils::controller_helper::ControllerHelper;
use crate::test_utils::{mock_app as mock_app_helper, AppExtension};

#[cfg(test)]
mod test_utils;

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";
const USER4: &str = "user4";
const USER5: &str = "user5";
const USER6: &str = "user6";
const USER7: &str = "user7";
const USER8: &str = "user8";
const USER9: &str = "user9";

struct PoolWithProxy {
    pool: (String, Uint128),
    proxy: Option<Addr>,
}

#[test]
fn migrate_from_110_to_200_with_deposit() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);

    let mut tokens: Vec<Addr> = vec![];
    for symbol in 'a'..'l' {
        let token_name = String::from(symbol) + &*String::from("EUR");
        tokens.push(instantiate_token(
            &mut app,
            cny_eur_token_code_id,
            token_name.as_str(),
            None,
        ))
    }

    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);
    assert_eq!(11, tokens.len());

    let mut pairs: Vec<(Addr, Addr)> = vec![];
    for eur_token in tokens {
        let (pair_cny_eur, lp_cny_eur) = create_pair(
            &mut app,
            &factory_instance,
            None,
            None,
            [
                AssetInfo::Token {
                    contract_addr: cny_token.clone(),
                },
                AssetInfo::Token {
                    contract_addr: eur_token.clone(),
                },
            ],
        );
        pairs.push((pair_cny_eur, lp_cny_eur));
    }

    let generator_instance =
        instantiate_generator_wth_version(&mut app, &astro_token_instance, None, "1.1.0");

    for pair in pairs.clone() {
        register_lp_tokens_in_generator_v110(
            &mut app,
            &generator_instance,
            vec![PoolWithProxy {
                pool: (pair.1.to_string(), Uint128::from(50u32)),
                proxy: None,
            }],
        );
    }

    for pair in pairs.clone() {
        let msg_cny_eur = QueryMsg::PoolInfo {
            lp_token: pair.1.to_string(),
        };

        // Check if proxy reward is none
        let reps: astroport_110::PoolInfoResponse = app
            .wrap()
            .query_wasm_smart(&generator_instance, &msg_cny_eur)
            .unwrap();
        assert_eq!(None, reps.reward_proxy);
    }

    let msg_cny_eur = astroport_110::QueryMsg::PoolLength {};
    let reps: astroport_110::PoolLengthResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(11, reps.length);

    for pair in pairs.clone() {
        // Mint and deposit LP tokens
        for user in [USER1, USER2] {
            mint_tokens(
                &mut app,
                pair.0.clone(),
                &pair.1,
                &Addr::unchecked(user),
                10,
            );
            deposit_lp_tokens_to_generator(&mut app, &generator_instance, user, &[(&pair.1, 10)]);
        }

        check_token_balance(&mut app, &pair.1, &generator_instance, 20);
    }

    app.update_block(|bi| next_block(bi));

    let new_generator_code_id = setup_generator_code_200(&mut app);
    let whitelist_code_id = store_whitelist_code(&mut app);

    // Migrate generator contract 1.1.0 -> 2.0.0
    app.migrate_contract(
        owner.clone(),
        generator_instance.clone(),
        &MigrateMsg {
            whitelist_code_id: Some(whitelist_code_id),
            factory: Option::from(factory_instance.to_string()),
            generator_controller: None,
            blocked_list_tokens: None,
            guardian: None,
            voting_escrow: None,
            generator_limit: None,
        },
        new_generator_code_id,
    )
    .unwrap();

    // check size of pools after migration
    let reps: astroport_110::PoolLengthResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &astroport_110::QueryMsg::PoolLength {})
        .unwrap();
    assert_eq!(11, reps.length);

    for pair in pairs.clone() {
        // Mint tokens, so user can deposit
        mint_tokens(&mut app, pair.0.clone(), &pair.1, &user1, 10);

        // deposit lp after migration
        deposit_lp_tokens_to_generator(&mut app, &generator_instance, USER1, &[(&pair.1, 10)]);
    }

    for pair in pairs.clone() {
        // check balance 20 before migration and 10 after
        check_token_balance(&mut app, &pair.1, &generator_instance, 30);
    }
}

#[test]
fn test_boost_checkpoints() {
    let mut app = mock_app_helper();
    let owner = Addr::unchecked("owner");
    let helper_controller = ControllerHelper::init(&mut app, &owner);

    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let cny_eur_token_code_id = store_token_code(&mut app);

    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &helper_controller.factory,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    register_lp_tokens_in_generator(
        &mut app,
        &helper_controller.generator,
        vec![PoolWithProxy {
            pool: (lp_cny_eur.to_string(), Uint128::from(100u32)),
            proxy: None,
        }],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 10);

    // Create short lock user1
    helper_controller
        .escrow_helper
        .mint_xastro(&mut app, USER1, 100);

    helper_controller
        .escrow_helper
        .create_lock(&mut app, USER1, WEEK * 3, 100f32)
        .unwrap();

    deposit_lp_tokens_to_generator(
        &mut app,
        &helper_controller.generator,
        USER1,
        &[(&lp_cny_eur, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &helper_controller.generator, 10);

    // check if virtual amount equal to 10
    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user1,
        10,
    );

    // Mint tokens, so user2 can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user2, 10);

    // Create short lock user2
    helper_controller
        .escrow_helper
        .mint_xastro(&mut app, USER2, 100);

    helper_controller
        .escrow_helper
        .create_lock(&mut app, USER2, WEEK * 3, 100f32)
        .unwrap();

    deposit_lp_tokens_to_generator(
        &mut app,
        &helper_controller.generator,
        USER2,
        &[(&lp_cny_eur, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &helper_controller.generator, 20);

    // check if virtual amount equal to 10
    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user2,
        10,
    );

    let err = app
        .execute_contract(
            Addr::unchecked(USER1),
            helper_controller.generator.clone(),
            &ExecuteMsg::CheckpointUserBoost {
                generators: vec![lp_cny_eur.to_string(); 26],
                user: Some(USER1.to_string()),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Maximum generator limit exceeded!",
        err.root_cause().to_string()
    );

    app.execute_contract(
        Addr::unchecked(USER1),
        helper_controller.generator.clone(),
        &ExecuteMsg::CheckpointUserBoost {
            generators: vec![lp_cny_eur.to_string()],
            user: Some(USER1.to_string()),
        },
        &[],
    )
    .unwrap();

    // check user1's ASTRO balance
    check_token_balance(
        &mut app,
        &helper_controller.escrow_helper.astro_token,
        &user1,
        0,
    );

    // check user2's ASTRO balance
    check_token_balance(
        &mut app,
        &helper_controller.escrow_helper.astro_token,
        &user2,
        0,
    );

    app.next_block(WEEK);

    app.execute_contract(
        Addr::unchecked(USER1),
        helper_controller.generator.clone(),
        &ExecuteMsg::Withdraw {
            lp_token: lp_cny_eur.to_string(),
            amount: Uint128::new(5),
        },
        &[],
    )
    .unwrap();

    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user1,
        5,
    );

    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user2,
        10,
    );

    // recalculate virtual amount for user2
    app.execute_contract(
        Addr::unchecked(USER2),
        helper_controller.generator.clone(),
        &ExecuteMsg::CheckpointUserBoost {
            generators: vec![lp_cny_eur.to_string()],
            user: Some(USER2.to_string()),
        },
        &[],
    )
    .unwrap();

    // check virtual amount for user2
    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user2,
        8,
    );

    // check user1's ASTRO balance after withdraw
    check_token_balance(
        &mut app,
        &helper_controller.escrow_helper.astro_token,
        &user1,
        5_000_000,
    );

    check_pending_rewards(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        USER1,
        (0, None),
    );

    check_pending_rewards(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        USER2,
        (0, None),
    );

    app.next_block(WEEK);

    app.execute_contract(
        Addr::unchecked(USER2),
        helper_controller.generator.clone(),
        &ExecuteMsg::Withdraw {
            lp_token: lp_cny_eur.to_string(),
            amount: Uint128::new(5),
        },
        &[],
    )
    .unwrap();

    check_pending_rewards(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        USER2,
        (0, None),
    );

    check_pending_rewards(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        USER1,
        (3_846_153, None),
    );

    check_token_balance(
        &mut app,
        &helper_controller.escrow_helper.astro_token,
        &user1,
        5_000_000,
    );

    // check user2's ASTRO balance after withdraw and checkpoint
    check_token_balance(
        &mut app,
        &helper_controller.escrow_helper.astro_token,
        &user2,
        11_153_846,
    );

    // check virtual amount for user2 after withdraw
    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user2,
        5,
    );

    // check virtual amount for user1
    check_emission_balance(
        &mut app,
        &helper_controller.generator,
        &lp_cny_eur,
        &user1,
        5,
    );
}

#[test]
fn proper_deposit_and_withdraw() {
    let mut app = mock_app();

    let user1 = Addr::unchecked(USER1);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);

    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let usd_token = instantiate_token(&mut app, cny_eur_token_code_id, "USD", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let (pair_eur_usd, lp_eur_usd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: usd_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_usd.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
        ],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 10);
    mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user1, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);

    check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER1, (0, None));
    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));

    app.update_block(|bi| next_block(bi));

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (5000000, None),
    );

    app.update_block(|bi| next_block(bi));

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 0);

    check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER1, (0, None));

    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));

    app.update_block(|bi| next_block(bi));

    check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER1, (0, None));

    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));
}

#[test]
fn set_tokens_per_block() {
    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    let msg = QueryMsg::Config {};
    let res: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.tokens_per_block, Uint128::new(10_000000));

    // Set new amount of tokens distributed per block
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
    let res: Config = app
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

    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    let msg = QueryMsg::Config {};
    let res: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.owner, OWNER);
    assert_eq!(res.generator_controller, Some(Addr::unchecked(OWNER)));
    assert_eq!(res.astro_token.to_string(), "contract0");
    assert_eq!(res.factory.to_string(), "contract1");
    assert_eq!(res.vesting_contract.to_string(), "contract2");

    let new_vesting = Addr::unchecked("new_vesting");

    let msg = ExecuteMsg::UpdateConfig {
        vesting_contract: Some(new_vesting.to_string()),
        generator_controller: None,
        guardian: None,
        voting_escrow: None,
        checkpoint_generator_limit: None,
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

    assert_eq!(e.root_cause().to_string(), "Unauthorized");

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = QueryMsg::Config {};
    let res: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.vesting_contract, new_vesting);
}

#[test]
fn update_owner() {
    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            generator_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            generator_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            generator_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        generator_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the state
    let msg = QueryMsg::Config {};
    let res: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();

    assert_eq!(res.owner.to_string(), new_owner)
}

#[test]
fn disabling_pool() {
    let mut app = mock_app();

    let user1 = Addr::unchecked(USER1);
    let owner = Addr::unchecked(OWNER);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let eur_usdt_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, eur_usdt_token_code_id, "EUR", None);
    let usdt_token = instantiate_token(&mut app, eur_usdt_token_code_id, "USDT", None);

    let (pair_eur_usdt, lp_eur_usdt) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: usdt_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    // Disable generator
    let msg = FactoryExecuteMsg::UpdatePairConfig {
        config: PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: true,
        },
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_eur_usdt.clone(), &lp_eur_usdt, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    let resp = app
        .execute_contract(user1.clone(), lp_eur_usdt.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(resp.root_cause().to_string(), "Generator is disabled!");

    // Enable generator
    let msg = FactoryExecuteMsg::UpdatePairConfig {
        config: PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        },
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    // Register LP token
    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![PoolWithProxy {
            pool: (lp_eur_usdt.to_string(), Uint128::from(10u32)),
            proxy: None,
        }],
    );

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), lp_eur_usdt.clone(), &msg, &[])
        .unwrap();
}

#[test]
fn generator_without_reward_proxies() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let usd_token = instantiate_token(&mut app, cny_eur_token_code_id, "USD", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let (pair_eur_usd, lp_eur_usd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: usd_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_usd.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
        ],
    );

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 9);
    mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    assert_eq!(
        app.execute_contract(user1.clone(), lp_cny_eur.clone(), &msg, &[])
            .unwrap_err()
            .root_cause()
            .to_string(),
        "Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 1);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);

    check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER1, (0, None));
    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));

    // User can't withdraw if they didn't deposit
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .root_cause()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // User can't emergency withdraw if they didn't deposit
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur.to_string(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .root_cause()
            .to_string(),
        "astroport::generator::UserInfo not found".to_string()
    );

    app.update_block(|bi| next_block(bi));

    // 10 tokens per block split equally between 2 pools
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user2, 10);
    mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user2, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER2,
        &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 20);

    // 10 tokens have been distributed to depositors since the last deposit
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (5_000000, None),
    );

    // New deposits can't receive already calculated rewards
    check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER2, (0, None));
    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER2, (0, None));

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_eur_usd.to_string(), Uint128::from(40u32)),
        ],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (8_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER2,
        (3_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already accrued rewards (5).
    // Pending tokens (3) will be redistributed to other staked users.
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur.to_string(),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (0_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER2,
        (3_000000, None),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER2,
        (2_000000, None),
    );

    // Balance of the generator should be decreased
    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 10);

    // User1 can't withdraw after emergency withdraw
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .root_cause()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    // User2 withdraw and get rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 3_000000);
    // 7 + 2 distributed ASTRO (for other pools). 5 orphaned by emergency withdrawals, 6 transfered to User2

    // User1 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User1 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // User2 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 5_000000);
}

// TODO: Mirror protocol does not support Terra2 and cosmwasm-1.0 respectively.
// TODO: Once astroport finds proxy reward contract this test should be updated.
/*#[test]
fn generator_with_mirror_reward_proxy() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let usd_token = instantiate_token(&mut app, cny_eur_token_code_id, "USD", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let (pair_eur_usd, lp_eur_usd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: usd_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    let (mirror_token_instance, mirror_staking_instance) =
        instantiate_mirror_protocol(&mut app, token_code_id, &pair_cny_eur, &lp_cny_eur);

    let proxy_code_id = store_proxy_code(&mut app);

    let proxy_to_mirror_instance = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &generator_instance,
        &pair_cny_eur,
        &lp_cny_eur,
        &mirror_staking_instance,
        &mirror_token_instance,
    );

    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(50u64)),
            (lp_eur_usd.to_string(), Uint128::from(50u64)),
        ],
    };

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Can't add if proxy isn't allowed
    let msg = GeneratorExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
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

    let msg = GeneratorExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
    };

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 9);
    mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user1, 10);

    let msg = Cw20ExecuteMsg::Send {
        contract: generator_instance.to_string(),
        msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
        amount: Uint128::new(10),
    };

    assert_eq!(
        app.execute_contract(user1.clone(), lp_cny_eur.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Overflow: Cannot Sub with 9 and 10".to_string()
    );

    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 1);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER1,
        &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    );

    // With the proxy, the Generator contract doesn't have the deposited LP tokens
    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    // The LP tokens are in the 3rd party contract now
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 10);

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd, &mirror_staking_instance, 0);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (0, Some(vec![0])),
    );
    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));

    // User can't withdraw if they didn't deposit previously
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string()
    );

    // User can't emergency withdraw if they didn't deposit previously
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur.to_string(),
    };
    assert_eq!(
        app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "astroport::generator::UserInfo not found".to_string()
    );

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(50_000000))],
        })
        .unwrap(),
        amount: Uint128::new(50_000000),
    };

    mint_tokens(
        &mut app,
        owner.clone(),
        &mirror_token_instance,
        &owner,
        50_000000,
    );
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    // 10 per block deposited equally between 2 pools with the same alloc_points
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5_000000, Some(vec![50_000000])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (5_000000, None),
    );

    // User 2
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user2, 10);
    mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user2, 10);

    deposit_lp_tokens_to_generator(
        &mut app,
        &generator_instance,
        USER2,
        &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    );

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 20);

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 20);
    check_token_balance(&mut app, &lp_eur_usd, &mirror_staking_instance, 0);

    // 10 tokens distributed to depositors since the last deposit
    // 5 distrubuted to proxy contract sicne the last deposit
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        50_000000,
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5_000000, Some(vec![50_000000])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (5_000000, None),
    );

    // New deposits can't receive already calculated rewards
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER2,
        (0, Some(vec![0])),
    );
    check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER2, (0, None));

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::new(60)),
            (lp_eur_usd.to_string(), Uint128::new(40)),
        ],
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(60_000000))],
        })
        .unwrap(),
        amount: Uint128::new(60_000000),
    };

    mint_tokens(
        &mut app,
        owner.clone(),
        &mirror_token_instance,
        &owner,
        60_000000,
    );
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    // 60 to cny_eur, 40 to eur_usd. Each is divided between two users
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (8_000000, Some(vec![80_000000])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER2,
        (3_000000, Some(vec![30_000000])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER2,
        (2_000000, None),
    );

    // User1 emergency withdraws and loses already distributed rewards (5).
    // Pending tokens (3) will be redistributed to other staked users.
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur.to_string(),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (0_000000, Some(vec![0])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER1,
        (7_000000, None),
    );

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER2,
        (3_000000, Some(vec![60_000000])),
    );
    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_eur_usd,
        USER2,
        (2_000000, None),
    );

    // Balance of the end contract should be decreased
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 10);

    // User1 can't withdraw after emergency withdrawal
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(1_000000),
    };
    assert_eq!(
        app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient balance in contract to process claim".to_string(),
    );

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        50_000000,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 0_000000);

    // Check if there are orphaned proxy rewards
    let msg = GeneratorQueryMsg::OrphanProxyRewards {
        lp_token: lp_cny_eur.to_string(),
    };
    let orphan_rewards: Vec<(AssetInfo, Uint128)> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();
    assert_eq!(orphan_rewards[0].1, Uint128::new(50_000000));

    // Owner sends orphaned proxy rewards
    let msg = GeneratorExecuteMsg::SendOrphanProxyReward {
        recipient: owner.to_string(),
        lp_token: lp_cny_eur.to_string(),
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0,
    );
    check_token_balance(&mut app, &mirror_token_instance, &owner, 50_000000);

    // Owner can't send proxy rewards for distribution to users
    let msg = GeneratorExecuteMsg::SendOrphanProxyReward {
        recipient: owner.to_string(),
        lp_token: lp_cny_eur.to_string(),
    };

    assert_eq!(
        app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
            .unwrap_err()
            .to_string(),
        "Insufficient amount of orphan rewards!"
    );

    // User2 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &user1, 10);
    check_token_balance(&mut app, &lp_cny_eur, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0);
    check_token_balance(&mut app, &astro_token_instance, &user2, 3_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    // 7 + 2 ASTRO were distributed (for other pools). 5 tokens were orphaned by the emergency withdrawal, 6 were transfered to User2
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );

    // User1 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 15);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 5);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User1 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(5),
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);

    // User2 withdraws and gets rewards
    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_eur_usd.to_string(),
        amount: Uint128::new(10),
    };
    app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 0);
    check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    check_token_balance(&mut app, &lp_eur_usd, &user2, 10);

    check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user1, 0_000000);
    check_token_balance(&mut app, &astro_token_instance, &user2, 5_000000);
    check_token_balance(&mut app, &mirror_token_instance, &user2, 60_000000);
    check_token_balance(
        &mut app,
        &mirror_token_instance,
        &proxy_to_mirror_instance,
        0_000000,
    );
}*/

#[test]
fn update_allowed_proxies() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);
    let allowed_proxies = Some(vec![
        "proxy1".to_string(),
        "proxy2".to_string(),
        "proxy3".to_string(),
        "proxy4".to_string(),
    ]);
    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        allowed_proxies,
        Some(OWNER.to_string()),
    );

    let msg = ExecuteMsg::UpdateAllowedProxies {
        add: None,
        remove: None,
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: Need to provide add or remove parameters",
        err.root_cause().to_string()
    );

    let msg = ExecuteMsg::UpdateAllowedProxies {
        add: Some(vec!["proxy5".to_string(), "proxy6".to_string()]),
        remove: Some(vec!["PROXY1".to_string(), "proxy3".to_string()]),
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Check if proxies were added and removed
    let reps: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &QueryMsg::Config {})
        .unwrap();

    let allowed_reward_proxies: Vec<Addr> = vec![
        Addr::unchecked("proxy2"),
        Addr::unchecked("proxy4"),
        Addr::unchecked("proxy5"),
        Addr::unchecked("proxy6"),
    ];
    assert_eq!(allowed_reward_proxies, reps.allowed_reward_proxies);

    // Check if proxies were removed already
    let msg = ExecuteMsg::UpdateAllowedProxies {
        add: None,
        remove: Some(vec!["proxy1".to_string(), "proxy2".to_string()]),
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: Can't remove proxy contract. It is not found in allowed list.",
        err.root_cause().to_string()
    );

    // Only add proxies
    let msg = ExecuteMsg::UpdateAllowedProxies {
        add: Some(vec!["proxy1".to_string(), "proxy2".to_string()]),
        remove: None,
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();
    let reps: Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &QueryMsg::Config {})
        .unwrap();
    let allowed_reward_proxies: Vec<Addr> = vec![
        Addr::unchecked("proxy2"),
        Addr::unchecked("proxy4"),
        Addr::unchecked("proxy5"),
        Addr::unchecked("proxy6"),
        Addr::unchecked("proxy1"),
    ];
    assert_eq!(allowed_reward_proxies, reps.allowed_reward_proxies);
}

/*#[test]
fn move_to_proxy() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![PoolWithProxy {
            pool: (lp_cny_eur.to_string(), Uint128::from(50u32)),
            proxy: None,
        }],
    );

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if proxy reward is none
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(None, reps.reward_proxy);

    let (mirror_token_instance, mirror_staking_instance) =
        instantiate_mirror_protocol(&mut app, token_code_id, &pair_cny_eur, &lp_cny_eur);

    let proxy_code_id = store_proxy_code(&mut app);

    let proxy_to_mirror_instance = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &generator_instance,
        &pair_cny_eur,
        &lp_cny_eur,
        &mirror_staking_instance,
        &mirror_token_instance,
    );

    // Can't add proxy if proxy reward isn't allowed
    let msg = ExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
    };
    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!("Reward proxy not allowed!", err.root_cause().to_string());

    let msg = GeneratorExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string()],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Set the proxy for the pool
    let msg = ExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if proxy reward exists
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Some(Addr::unchecked("contract #11")), reps.reward_proxy);

    // Mint tokens, so user can deposit
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 10);

    deposit_lp_tokens_to_generator(&mut app, &generator_instance, USER1, &[(&lp_cny_eur, 10)]);

    // With the proxy set up, the Generator contract doesn't have the deposited LP tokens
    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    // The LP tokens are in the 3rd party contract now
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 10);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (0, Some(vec![0])),
    );

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(50_000000))],
        })
        .unwrap(),
        amount: Uint128::new(50_000000),
    };

    mint_tokens(
        &mut app,
        owner.clone(),
        &mirror_token_instance,
        &owner,
        50_000000,
    );
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (10_000000, Some(vec![50_000000])),
    );

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 10);

    // Check if the pool already has a reward proxy contract set
    let msg = ExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
    };
    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "The pool already has a reward proxy contract!",
        err.root_cause().to_string()
    )
}*/

/*#[test]
fn migrate_proxy() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let generator_instance = instantiate_generator_wth_version(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        "1.2.0",
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![PoolWithProxy {
            pool: (lp_cny_eur.to_string(), Uint128::from(50u32)),
            proxy: None,
        }],
    );

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if proxy reward is none
    let reps: generator_v120::PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(None, reps.reward_proxy);

    let (mirror_token_instance, mirror_staking_instance) =
        instantiate_mirror_protocol(&mut app, token_code_id, &pair_cny_eur, &lp_cny_eur);

    let proxy_code_id = store_proxy_code(&mut app);

    let proxy_to_mirror_instance = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &generator_instance,
        &pair_cny_eur,
        &lp_cny_eur,
        &mirror_staking_instance,
        &mirror_token_instance,
    );

    let msg = generator_v120::ExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string()],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Set the proxy for the pool
    let msg = generator_v120::ExecuteMsg::MoveToProxy {
        lp_token: lp_cny_eur.to_string(),
        proxy: proxy_to_mirror_instance.to_string(),
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Mint and deposit LP tokens
    for user in [USER1, USER2] {
        mint_tokens(
            &mut app,
            pair_cny_eur.clone(),
            &lp_cny_eur,
            &Addr::unchecked(user),
            10,
        );
        deposit_lp_tokens_to_generator(&mut app, &generator_instance, user, &[(&lp_cny_eur, 10)]);
    }

    // With the proxy set up, the Generator contract doesn't have the deposited LP tokens
    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    // The LP tokens are in the 3rd party contract now
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 20);

    for user in [USER1, USER2] {
        check_pending_rewards_v120(
            &mut app,
            &generator_instance,
            &lp_cny_eur,
            user,
            (0, Some(0)),
        );
    }

    app.update_block(|bi| next_block(bi));

    let msg = Cw20ExecuteMsg::Send {
        contract: mirror_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(50_000000))],
        })
        .unwrap(),
        amount: Uint128::new(50_000000),
    };

    mint_tokens(
        &mut app,
        owner.clone(),
        &mirror_token_instance,
        &owner,
        50_000000,
    );
    app.execute_contract(owner.clone(), mirror_token_instance.clone(), &msg, &[])
        .unwrap();

    for user in [USER1, USER2] {
        check_pending_rewards_v120(
            &mut app,
            &generator_instance,
            &lp_cny_eur,
            user,
            (5_000000, Some(25_000000)),
        );
    }

    let claim_msg = ExecuteMsg::ClaimRewards {
        lp_tokens: vec![lp_cny_eur.to_string()],
    };
    // user2 claims rewards
    app.execute_contract(user2.clone(), generator_instance.clone(), &claim_msg, &[])
        .unwrap();
    check_token_balance(&mut app, &mirror_token_instance, &user2, 25_000000);

    let new_generator_code_id = setup_generator_code(&mut app);

    let whitelist_code_id = store_whitelist_code(&mut app);

    // Migrate generator contract 1.2.0 -> 1.3.0
    app.migrate_contract(
        owner.clone(),
        generator_instance.clone(),
        &MigrateMsg {
            whitelist_code_id: Some(whitelist_code_id),
            factory: None,
            generator_controller: None,
            blocked_list_tokens: None,
            guardian: None,
            voting_escrow: None,
            generator_limit: None,
        },
        new_generator_code_id,
    )
    .unwrap();

    let (foo_token, foo_token_staking_instance) =
        instantiate_mirror_protocol(&mut app, token_code_id, &pair_cny_eur, &lp_cny_eur);

    let foo_proxy = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &generator_instance,
        &pair_cny_eur,
        &lp_cny_eur,
        &foo_token_staking_instance,
        &foo_token,
    );

    let msg = ExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![proxy_to_mirror_instance.to_string(), foo_proxy.to_string()],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Cannot migrate to the same proxy
    let msg = ExecuteMsg::MigrateProxy {
        lp_token: lp_cny_eur.to_string(),
        new_proxy: proxy_to_mirror_instance.to_string(),
    };
    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Can not migrate to the same proxy"
    );

    // Migrate to another reward proxy
    let msg = ExecuteMsg::MigrateProxy {
        lp_token: lp_cny_eur.to_string(),
        new_proxy: foo_proxy.to_string(),
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Check LP tokens have been successfully transferred and deposited
    check_token_balance(&mut app, &lp_cny_eur, &mirror_staking_instance, 0);
    check_token_balance(&mut app, &lp_cny_eur, &foo_token_staking_instance, 20);

    check_pending_rewards(
        &mut app,
        &generator_instance,
        &lp_cny_eur,
        USER1,
        (5_000000, Some(vec![25_000000, 0])),
    );

    // Simulate foo proxy reward income
    let reward_amount = 100_000000;
    let msg = Cw20ExecuteMsg::Send {
        contract: foo_token_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(reward_amount))],
        })
        .unwrap(),
        amount: Uint128::new(reward_amount),
    };

    mint_tokens(&mut app, owner.clone(), &foo_token, &owner, reward_amount);
    app.execute_contract(owner.clone(), foo_token.clone(), &msg, &[])
        .unwrap();

    app.update_block(next_block);

    let (bar_token, bar_token_staking_instance) =
        instantiate_mirror_protocol(&mut app, token_code_id, &pair_cny_eur, &lp_cny_eur);

    let bar_proxy = instantiate_proxy(
        &mut app,
        proxy_code_id,
        &generator_instance,
        &pair_cny_eur,
        &lp_cny_eur,
        &bar_token_staking_instance,
        &bar_token,
    );

    let msg = GeneratorExecuteMsg::SetAllowedRewardProxies {
        proxies: vec![foo_proxy.to_string(), bar_proxy.to_string()],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Migrate to the 3rd reward proxy
    let msg = ExecuteMsg::MigrateProxy {
        lp_token: lp_cny_eur.to_string(),
        new_proxy: bar_proxy.to_string(),
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Simulate bar proxy reward income
    let reward_amount = 70_000000;
    let msg = Cw20ExecuteMsg::Send {
        contract: bar_token_staking_instance.to_string(),
        msg: to_binary(&MirrorStakingHookMsg::DepositReward {
            rewards: vec![(pair_cny_eur.to_string(), Uint128::new(reward_amount))],
        })
        .unwrap(),
        amount: Uint128::new(reward_amount),
    };

    mint_tokens(&mut app, owner.clone(), &bar_token, &owner, reward_amount);
    app.execute_contract(owner.clone(), bar_token.clone(), &msg, &[])
        .unwrap();

    // Claim all rewards twice to be sure that nothing breaks and users do not receive additional rewards
    app.execute_contract(user1.clone(), generator_instance.clone(), &claim_msg, &[])
        .unwrap();
    app.execute_contract(user1.clone(), generator_instance.clone(), &claim_msg, &[])
        .unwrap();
    check_token_balance(&mut app, &mirror_token_instance, &user1, 25_000000);
    check_token_balance(&mut app, &foo_token, &user1, 50_000000);
    check_token_balance(&mut app, &bar_token, &user1, 35_000000);

    // Check emergency withdraw
    let msg = GeneratorExecuteMsg::EmergencyWithdraw {
        lp_token: lp_cny_eur.to_string(),
    };
    app.execute_contract(user2, generator_instance.clone(), &msg, &[])
        .unwrap();
    let msg = GeneratorQueryMsg::OrphanProxyRewards {
        lp_token: lp_cny_eur.to_string(),
    };
    let orphan_rewards: Vec<(AssetInfo, Uint128)> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();
    assert_eq!(
        orphan_rewards,
        vec![
            (
                AssetInfo::Token {
                    contract_addr: mirror_token_instance
                },
                Uint128::new(0) // user2 has already claimed mirror token rewards
            ),
            (
                AssetInfo::Token {
                    contract_addr: foo_token
                },
                Uint128::new(50_000000)
            ),
            (
                AssetInfo::Token {
                    contract_addr: bar_token
                },
                Uint128::new(35_000000)
            )
        ]
    );
}*/

#[test]
fn query_all_stakers() {
    let mut app = mock_app();

    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let user3 = Addr::unchecked(USER3);
    let user4 = Addr::unchecked(USER4);
    let user5 = Addr::unchecked(USER5);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![PoolWithProxy {
            pool: (lp_cny_eur.to_string(), Uint128::new(100)),
            proxy: None,
        }],
    );

    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 10);
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user2, 10);
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user3, 10);
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user4, 10);
    mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user5, 10);

    let msg_cny_eur = QueryMsg::PoolStakers {
        lp_token: lp_cny_eur.to_string(),
        start_after: None,
        limit: None,
    };

    // Check there are no stakers when there's no deposit
    let reps: Vec<StakerResponse> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    let empty: Vec<StakerResponse> = vec![];
    assert_eq!(empty, reps);

    for user in [USER1, USER2, USER3, USER4, USER5] {
        deposit_lp_tokens_to_generator(&mut app, &generator_instance, user, &[(&lp_cny_eur, 10)]);
    }

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 50);

    let reps: Vec<StakerResponse> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();

    assert_eq!(
        vec![
            StakerResponse {
                account: "user1".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user2".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user3".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user4".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user5".to_string(),
                amount: Uint128::new(10)
            }
        ],
        reps
    );

    let msg = GeneratorExecuteMsg::Withdraw {
        lp_token: lp_cny_eur.to_string(),
        amount: Uint128::new(10),
    };

    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 40);

    // Check the amount of stakers after withdrawal
    let reps: Vec<StakerResponse> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();

    assert_eq!(
        vec![
            StakerResponse {
                account: "user2".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user3".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user4".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user5".to_string(),
                amount: Uint128::new(10)
            }
        ],
        reps
    );
}

#[test]
fn query_pagination_stakers() {
    let mut app = mock_app();

    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let user3 = Addr::unchecked(USER3);
    let user4 = Addr::unchecked(USER4);
    let user5 = Addr::unchecked(USER5);
    let user6 = Addr::unchecked(USER6);
    let user7 = Addr::unchecked(USER7);
    let user8 = Addr::unchecked(USER8);
    let user9 = Addr::unchecked(USER9);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        None,
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![PoolWithProxy {
            pool: (lp_cny_eur.to_string(), Uint128::from(100u32)),
            proxy: None,
        }],
    );

    for user in [
        user1, user2, user3, user4, user5, user6, user7, user8, user9,
    ] {
        mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user, 10);
    }

    for user in [
        USER1, USER2, USER3, USER4, USER5, USER6, USER7, USER8, USER9,
    ] {
        deposit_lp_tokens_to_generator(&mut app, &generator_instance, user, &[(&lp_cny_eur, 10)]);
    }

    check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 90);

    // Get the first two stakers
    let msg_cny_eur = QueryMsg::PoolStakers {
        lp_token: lp_cny_eur.to_string(),
        start_after: None,
        limit: Some(2),
    };

    let reps: Vec<StakerResponse> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();

    // check count of users
    assert_eq!(reps.len(), 2 as usize);

    assert_eq!(
        vec![
            StakerResponse {
                account: "user1".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user2".to_string(),
                amount: Uint128::new(10)
            },
        ],
        reps
    );

    // Get the next seven stakers
    let msg_cny_eur = QueryMsg::PoolStakers {
        lp_token: lp_cny_eur.to_string(),
        start_after: Some("user2".to_string()),
        limit: Some(7),
    };

    let reps: Vec<StakerResponse> = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();

    assert_eq!(
        vec![
            StakerResponse {
                account: "user3".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user4".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user5".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user6".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user7".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user8".to_string(),
                amount: Uint128::new(10)
            },
            StakerResponse {
                account: "user9".to_string(),
                amount: Uint128::new(10)
            },
        ],
        reps
    );
}

#[test]
fn update_tokens_blocked_list() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    let cny_token = instantiate_token(&mut app, token_code_id, "CNY", None);
    let eur_token = instantiate_token(&mut app, token_code_id, "EUR", None);
    let ukr_token = instantiate_token(&mut app, token_code_id, "UKR", None);
    let msi_token = instantiate_token(&mut app, token_code_id, "MSI", None);

    let (_, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let (_, lp_cny_ukr) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: ukr_token.clone(),
            },
        ],
    );

    let (_, lp_eur_msi) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: msi_token.clone(),
            },
        ],
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_cny_ukr.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_msi.to_string(), Uint128::new(100)),
                proxy: None,
            },
        ],
    );

    let msg = ExecuteMsg::UpdateBlockedTokenslist {
        add: None,
        remove: None,
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: Need to provide add or remove parameters",
        err.root_cause().to_string()
    );

    let msg = ExecuteMsg::UpdateBlockedTokenslist {
        add: Some(vec![native_asset_info("uusd".to_string())]),
        remove: None,
    };

    let err = app
        .execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "ASTRO or Terra native assets (UST, LUNA etc) cannot be blocked!",
        err.root_cause().to_string()
    );

    let msg = ExecuteMsg::UpdateBlockedTokenslist {
        add: Some(vec![token_asset_info(cny_token.clone())]),
        remove: None,
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // check if we cannot change the allocation point for blocked token
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_cny_ukr.to_string(), Uint128::from(40u32)),
            (lp_eur_msi.to_string(), Uint128::from(140u32)),
        ],
    };
    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        format!("Generic error: Token {} is blocked!", cny_token.to_string()),
        err.root_cause().to_string()
    );

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![(lp_eur_msi.to_string(), Uint128::from(140u32))],
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_ukr.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_eur_msi.to_string(),
    };

    // Check if alloc point is equal to 140
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(140), reps.alloc_point);

    let msg = ExecuteMsg::UpdateBlockedTokenslist {
        add: None,
        remove: Some(vec![native_asset_info("eur".to_string())]),
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: Can't remove token. It is not found in the blocked list.",
        err.root_cause().to_string()
    );

    let msg = ExecuteMsg::UpdateBlockedTokenslist {
        add: None,
        remove: Some(vec![token_asset_info(cny_token)]),
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_cny_ukr.to_string(), Uint128::from(40u32)),
            (lp_eur_msi.to_string(), Uint128::from(140u32)),
        ],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 60
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(60), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_ukr.to_string(),
    };

    // Check if alloc point is equal to 40
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(40), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_eur_msi.to_string(),
    };

    // Check if alloc point is equal to 140
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(140), reps.alloc_point);
}

#[test]
fn setup_pools() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    // add generator to factory
    let msg = FactoryExecuteMsg::UpdateConfig {
        token_code_id: None,
        fee_address: None,
        generator_address: Some(generator_instance.to_string()),
        whitelist_code_id: None,
    };

    app.execute_contract(Addr::unchecked(OWNER), factory_instance.clone(), &msg, &[])
        .unwrap();

    let res: FactoryConfigResponse = app
        .wrap()
        .query_wasm_smart(&factory_instance.clone(), &FactoryQueryMsg::Config {})
        .unwrap();

    assert_eq!(res.generator_address, Some(generator_instance.clone()));

    let (_, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "eur".to_string(),
            },
        ],
    );

    let (_, lp_cny_uusd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    );

    let (_, lp_eur_uusd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::NativeToken {
                denom: "eur".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_cny_uusd.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_uusd.to_string(), Uint128::new(100)),
                proxy: None,
            },
        ],
    );

    // deregister pair and set the allocation point to zero for pool
    app.execute_contract(
        Addr::unchecked(OWNER),
        factory_instance.clone(),
        &FactoryExecuteMsg::Deregister {
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: "cny".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "eur".to_string(),
                },
            ],
        },
        &[],
    )
    .unwrap();

    // Check if the allocation point for lp_cny_eur is equal to zero
    let res: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(
            generator_instance.to_owned(),
            &GeneratorQueryMsg::PoolInfo {
                lp_token: lp_cny_eur.to_string(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::zero(), res.alloc_point);

    // Check pool length
    // let res: usize = app
    //     .wrap()
    //     .query_wasm_smart(
    //         generator_instance.to_owned(),
    //         &GeneratorQueryMsg::ActivePoolLength {},
    //     )
    //     .unwrap();
    // assert_eq!(3, res);

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_eur_uusd.to_string(), Uint128::from(40u32)),
            (lp_cny_uusd.to_string(), Uint128::from(140u32)),
        ],
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: The pair aren't registered: cny-eur",
        err.root_cause().to_string()
    );

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_eur_uusd.to_string(), Uint128::from(40u32)),
            (lp_cny_uusd.to_string(), Uint128::from(140u32)),
        ],
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_uusd.to_string(),
    };

    // Check if alloc point is equal to 140
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(140), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_eur_uusd.to_string(),
    };

    // Check if alloc point is equal to 40
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(40), reps.alloc_point);

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_eur_uusd.to_string(), Uint128::from(80u32)),
            (lp_cny_uusd.to_string(), Uint128::from(80u32)),
        ],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_uusd.to_string(),
    };

    // Check if alloc point is equal to 80
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(80), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_eur_uusd.to_string(),
    };

    // Check if alloc point is equal to 80
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(80), reps.alloc_point);
}

#[test]
fn deactivate_pools_by_pair_types() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user1 = Addr::unchecked(USER1);
    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);
    let pair_stable_code_id = store_pair_stable_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let factory_instance = instantiate_factory(
        &mut app,
        factory_code_id,
        token_code_id,
        pair_code_id,
        Some(pair_stable_code_id),
    );

    let generator_instance = instantiate_generator(
        &mut app,
        &factory_instance,
        &astro_token_instance,
        None,
        Some(OWNER.to_string()),
    );

    // add generator to factory
    let msg = FactoryExecuteMsg::UpdateConfig {
        token_code_id: None,
        fee_address: None,
        generator_address: Some(generator_instance.to_string()),
        whitelist_code_id: None,
    };

    app.execute_contract(Addr::unchecked(OWNER), factory_instance.clone(), &msg, &[])
        .unwrap();

    let res: FactoryConfigResponse = app
        .wrap()
        .query_wasm_smart(&factory_instance.clone(), &FactoryQueryMsg::Config {})
        .unwrap();

    assert_eq!(res.generator_address, Some(generator_instance.clone()));

    let (_, lp_cny_uusd) = create_pair(
        &mut app,
        &factory_instance,
        Some(PairType::Stable {}),
        Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
        [
            AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    );

    let (_, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "eur".to_string(),
            },
        ],
    );

    let (_, lp_eur_uusd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        [
            AssetInfo::NativeToken {
                denom: "eur".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    );

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_cny_uusd.to_string(), Uint128::new(100)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_uusd.to_string(), Uint128::new(100)),
                proxy: None,
            },
        ],
    );

    // try to deactivate pools for not blacklisted pair types
    let msg = GeneratorExecuteMsg::DeactivatePools {
        pair_types: vec![PairType::Xyk {}, PairType::Stable {}],
    };
    let err = app
        .execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        format!(
            "Generic error: Pair type ({}) is not blacklisted!",
            PairType::Xyk {}
        ),
        err.root_cause().to_string()
    );

    // Add stable pair type to blacklist
    let msg = FactoryExecuteMsg::UpdatePairConfig {
        config: PairConfig {
            code_id: pair_stable_code_id,
            pair_type: PairType::Stable {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: true,
        },
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    // check if we add stable pair type to blacklist
    let res: Vec<PairType> = app
        .wrap()
        .query_wasm_smart(
            &factory_instance.clone(),
            &FactoryQueryMsg::BlacklistedPairTypes {},
        )
        .unwrap();
    assert_eq!(res, vec![PairType::Stable {}]);

    let msg = GeneratorExecuteMsg::DeactivatePools {
        pair_types: vec![PairType::Stable {}],
    };
    app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_uusd.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    // try to change alloc point for blacklisted pool by pair type
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_eur_uusd.to_string(), Uint128::from(40u32)),
            (lp_cny_uusd.to_string(), Uint128::from(140u32)),
        ],
    };

    let err = app
        .execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: Pair type (stable) is blacklisted!",
        err.root_cause().to_string()
    );

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_cny_eur.to_string(), Uint128::from(60u32)),
            (lp_eur_uusd.to_string(), Uint128::from(40u32)),
        ],
    };

    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 60
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(60), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_uusd.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_eur_uusd.to_string(),
    };

    // Check if alloc point is equal to 40
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::new(40), reps.alloc_point);

    // remove stable pair type from blacklist
    let msg = FactoryExecuteMsg::UpdatePairConfig {
        config: PairConfig {
            code_id: pair_stable_code_id,
            pair_type: PairType::Stable {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        },
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    // check if we remove stable pair type from blacklist
    let res: Vec<PairType> = app
        .wrap()
        .query_wasm_smart(
            &factory_instance.clone(),
            &FactoryQueryMsg::BlacklistedPairTypes {},
        )
        .unwrap();
    assert_eq!(res, vec![]);

    // Change pool alloc points
    let msg = GeneratorExecuteMsg::SetupPools {
        pools: vec![
            (lp_eur_uusd.to_string(), Uint128::from(80u32)),
            (lp_cny_uusd.to_string(), Uint128::from(80u32)),
        ],
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg_cny_eur = QueryMsg::PoolInfo {
        lp_token: lp_cny_eur.to_string(),
    };

    // Check if alloc point is equal to 0
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_eur)
        .unwrap();
    assert_eq!(Uint128::zero(), reps.alloc_point);

    let msg_cny_uusd = QueryMsg::PoolInfo {
        lp_token: lp_cny_uusd.to_string(),
    };

    // Check if alloc point is equal to 80
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_cny_uusd)
        .unwrap();
    assert_eq!(Uint128::new(80), reps.alloc_point);

    let msg_eur_uusd = QueryMsg::PoolInfo {
        lp_token: lp_eur_uusd.to_string(),
    };

    // Check if alloc point is equal to 80
    let reps: PoolInfoResponse = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg_eur_uusd)
        .unwrap();
    assert_eq!(Uint128::new(80), reps.alloc_point);
}

fn mock_app() -> App {
    App::default()
}

fn store_token_code(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn store_factory_code(app: &mut App) -> u64 {
    let factory_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    );

    app.store_code(factory_contract)
}

fn store_pair_code_id(app: &mut App) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );

    app.store_code(pair_contract)
}

fn store_pair_stable_code_id(app: &mut App) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_stable::contract::execute,
            astroport_pair_stable::contract::instantiate,
            astroport_pair_stable::contract::query,
        )
        .with_reply_empty(astroport_pair_stable::contract::reply),
    );

    app.store_code(pair_contract)
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
        marketing: None,
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

fn instantiate_factory(
    app: &mut App,
    factory_code_id: u64,
    token_code_id: u64,
    pair_code_id: u64,
    pair_stable_code_id: Option<u64>,
) -> Addr {
    let mut msg = FactoryInstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id,
        fee_address: None,
        generator_address: None,
        owner: String::from(OWNER),
        whitelist_code_id: 0,
    };

    if let Some(pair_stable_code_id) = pair_stable_code_id {
        msg.pair_configs.push(PairConfig {
            code_id: pair_stable_code_id,
            pair_type: PairType::Stable {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        });
    }

    app.instantiate_contract(
        factory_code_id,
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "Factory",
        None,
    )
    .unwrap()
}

fn setup_generator_v110_code(app: &mut App) -> u64 {
    let generator_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_generator_v110::contract::execute,
            astroport_generator_v110::contract::instantiate,
            astroport_generator_v110::contract::query,
        )
        .with_migrate_empty(astroport_generator_v110::contract::migrate)
        .with_reply_empty(astroport_generator_v110::contract::reply),
    );

    app.store_code(generator_contract)
}

fn setup_generator_code_200(app: &mut App) -> u64 {
    let generator_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_generator::contract::execute,
            astroport_generator::contract::instantiate,
            astroport_generator::contract::query,
        )
        .with_migrate_empty(astroport_generator::contract::migrate)
        .with_reply_empty(astroport_generator::contract::reply),
    );

    app.store_code(generator_contract)
}

fn instantiate_generator(
    mut app: &mut App,
    factory_instance: &Addr,
    astro_token_instance: &Addr,
    allowed_proxies: Option<Vec<String>>,
    generator_controller: Option<String>,
) -> Addr {
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
        owner.clone(),
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

    let whitelist_code_id = store_whitelist_code(&mut app);
    let generator_code_id = app.store_code(generator_contract);

    let init_msg = GeneratorInstantiateMsg {
        owner: owner.to_string(),
        factory: factory_instance.to_string(),
        guardian: None,
        allowed_reward_proxies: allowed_proxies.unwrap_or_default(),
        start_block: Uint64::from(app.block_info().height),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::new(10_000000),
        vesting_contract: vesting_instance.to_string(),
        generator_controller,
        voting_escrow: None,
        whitelist_code_id,
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

    // Vesting to generator:
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

fn instantiate_generator_wth_version(
    mut app: &mut App,
    astro_token_instance: &Addr,
    allowed_proxies: Option<Vec<String>>,
    generator_version: &str,
) -> Addr {
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
        owner.clone(),
        &astro_token_instance,
        &owner,
        1_000_000_000_000000,
    );

    let generator_code_id = match generator_version {
        _ => setup_generator_v110_code(&mut app),
    };

    let init_msg = astroport_package_generator_v110::generator::InstantiateMsg {
        owner: owner.to_string(),
        allowed_reward_proxies: allowed_proxies.unwrap_or_default(),
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
            Some(OWNER.to_string()),
        )
        .unwrap();

    // Vesting to generator:
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

/*
fn instantiate_mirror_protocol(
    _app: &mut App,
    _token_code_id: u64,
    _asset_token: &Addr,
    _staking_token: &Addr,
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

    (Addr::unchecked(""), Addr::unchecked(""))
}


fn store_proxy_code(app: &mut App) -> u64 {
    let generator_proxy_to_mirror_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_generator_proxy_to_mirror::contract::execute,
        astroport_generator_proxy_to_mirror::contract::instantiate,
        astroport_generator_proxy_to_mirror::contract::query,
    ));

    app.store_code(generator_proxy_to_mirror_contract)
}

fn instantiate_proxy(
    app: &mut App,
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
*/

fn register_lp_tokens_in_generator_v110(
    app: &mut App,
    generator_instance: &Addr,
    pools_with_proxy: Vec<PoolWithProxy>,
) {
    for pool_with_proxy in &pools_with_proxy {
        app.execute_contract(
            Addr::unchecked(OWNER),
            generator_instance.clone(),
            &astroport_110::ExecuteMsg::Add {
                lp_token: pool_with_proxy.pool.0.clone().to_string(),
                alloc_point: Uint64::try_from(pool_with_proxy.pool.1).unwrap(),
                has_asset_rewards: false,
                reward_proxy: None,
            },
            &[],
        )
        .unwrap();
    }
}

fn register_lp_tokens_in_generator(
    app: &mut App,
    generator_instance: &Addr,
    pools_with_proxy: Vec<PoolWithProxy>,
) {
    let pools: Vec<(String, Uint128)> = pools_with_proxy.iter().map(|p| p.pool.clone()).collect();

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &GeneratorExecuteMsg::SetupPools { pools },
        &[],
    )
    .unwrap();

    for pool_with_proxy in &pools_with_proxy {
        if let Some(proxy) = &pool_with_proxy.proxy {
            app.execute_contract(
                Addr::unchecked(OWNER),
                generator_instance.clone(),
                &GeneratorExecuteMsg::MoveToProxy {
                    lp_token: pool_with_proxy.pool.0.clone(),
                    proxy: proxy.to_string(),
                },
                &[],
            )
            .unwrap();
        }
    }
}

fn mint_tokens(app: &mut App, sender: Addr, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(sender, token.to_owned(), &msg, &[])
        .unwrap();
}

fn deposit_lp_tokens_to_generator(
    app: &mut App,
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

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn check_emission_balance(
    app: &mut App,
    generator: &Addr,
    lp_token: &Addr,
    user: &Addr,
    expected: u128,
) {
    let msg = GeneratorQueryMsg::UserVirtualAmount {
        lp_token: lp_token.to_string(),
        user: user.to_string(),
    };

    let res: Uint128 = app.wrap().query_wasm_smart(generator, &msg).unwrap();
    assert_eq!(Uint128::from(expected), res);
}

fn check_pending_rewards(
    app: &mut App,
    generator_instance: &Addr,
    token: &Addr,
    depositor: &str,
    (expected, expected_on_proxy): (u128, Option<Vec<u128>>),
) {
    let msg = GeneratorQueryMsg::PendingToken {
        lp_token: token.to_string(),
        user: String::from(depositor),
    };

    let res: PendingTokenResponse = app
        .wrap()
        .query_wasm_smart(generator_instance.to_owned(), &msg)
        .unwrap();

    assert_eq!(res.pending.u128(), expected);
    let pending_on_proxy = res.pending_on_proxy.map(|rewards| {
        rewards
            .into_iter()
            .map(|Asset { amount, .. }| amount.u128())
            .collect::<Vec<_>>()
    });
    assert_eq!(pending_on_proxy, expected_on_proxy)
}

fn create_pair(
    app: &mut App,
    factory: &Addr,
    pair_type: Option<PairType>,
    init_param: Option<Binary>,
    assets: [AssetInfo; 2],
) -> (Addr, Addr) {
    app.execute_contract(
        Addr::unchecked(OWNER),
        factory.clone(),
        &FactoryExecuteMsg::CreatePair {
            pair_type: pair_type.unwrap_or_else(|| PairType::Xyk {}),
            asset_infos: assets.clone(),
            init_params: init_param,
        },
        &[],
    )
    .unwrap();

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            factory,
            &FactoryQueryMsg::Pair {
                asset_infos: assets,
            },
        )
        .unwrap();

    (res.contract_addr, res.liquidity_token)
}

fn store_whitelist_code(app: &mut App) -> u64 {
    let whitelist_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_whitelist::contract::execute,
        astroport_whitelist::contract::instantiate,
        astroport_whitelist::contract::query,
    ));

    app.store_code(whitelist_contract)
}
