#![cfg(not(tarpaulin_include))]

mod factory_helper;

use cosmwasm_std::{attr, Addr, StdError};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, PairConfig, PairType, QueryMsg,
    TrackerConfig,
};

use crate::factory_helper::{instantiate_token, FactoryHelper};
use astroport_factory::error::ContractError;
use astroport_test::cw_multi_test::{AppBuilder, ContractWrapper, Executor};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};

fn mock_app() -> TestApp {
    AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {})
}

fn store_factory_code(app: &mut TestApp) -> u64 {
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

#[test]
fn proper_initialization() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let factory_code_id = store_factory_code(&mut app);

    let pair_configs = vec![PairConfig {
        code_id: 321,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: false,
        is_generator_disabled: false,
        permissioned: false,
        whitelist: None,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id: 123,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: None,
    };

    let factory_instance = app
        .instantiate_contract(
            factory_code_id,
            Addr::unchecked(owner.clone()),
            &msg,
            &[],
            "factory",
            None,
        )
        .unwrap();

    let msg = QueryMsg::Config {};
    let config_res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&factory_instance, &msg)
        .unwrap();

    assert_eq!(123, config_res.token_code_id);
    assert_eq!(pair_configs, config_res.pair_configs);
    assert_eq!(owner, config_res.owner);
}

#[test]
fn update_config() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    // Update config
    helper
        .update_config(
            &mut app,
            &owner,
            Some(200u64),
            Some("fee".to_string()),
            Some("generator".to_string()),
            None,
            None,
        )
        .unwrap();

    let config_res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!("fee", config_res.fee_address.unwrap().to_string());
    assert_eq!(
        "generator",
        config_res.generator_address.unwrap().to_string()
    );

    // Unauthorized err
    let res = helper
        .update_config(
            &mut app,
            &Addr::unchecked("not_owner"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Unauthorized");
}

#[test]
fn test_create_pair() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    let err = helper
        .create_pair(&mut app, &owner, PairType::Xyk {}, [&token1, &token1], None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Doubling assets in asset infos"
    );

    let res = helper
        .create_pair(&mut app, &owner, PairType::Xyk {}, [&token1, &token2], None)
        .unwrap();

    let err = helper
        .create_pair(&mut app, &owner, PairType::Xyk {}, [&token1, &token2], None)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Pair was already created");

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
    );

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::Pair {
                asset_infos: vec![
                    AssetInfo::Token {
                        contract_addr: token1.clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: token2.clone(),
                    },
                ],
            },
        )
        .unwrap();

    // In multitest, contract names are counted in the order in which contracts are created
    assert_eq!("contract1", helper.factory.to_string());
    assert_eq!("contract4", res.contract_addr.to_string());
    assert_eq!(
        "factory/contract4/astroport/share",
        res.liquidity_token.to_string()
    );

    // Create disabled pair type
    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::UpdatePairConfig {
            config: PairConfig {
                code_id: 0,
                pair_type: PairType::Custom("Custom".to_string()),
                total_fee_bps: 100,
                maker_fee_bps: 40,
                is_disabled: true,
                is_generator_disabled: false,
                permissioned: false,
                whitelist: None,
            },
        },
        &[],
    )
    .unwrap();

    let token3 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    let err = helper
        .create_pair(
            &mut app,
            &Addr::unchecked("someone"),
            PairType::Custom("Custom".to_string()),
            [&token1, &token3],
            None,
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Pair config disabled");

    // Query fee info
    let fee_info: FeeInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::FeeInfo {
                pair_type: PairType::Custom("Custom".to_string()),
            },
        )
        .unwrap();
    assert_eq!(100, fee_info.total_fee_bps);
    assert_eq!(40, fee_info.maker_fee_bps);

    // query blacklisted pairs
    let pair_types: Vec<PairType> = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::BlacklistedPairTypes {})
        .unwrap();
    assert_eq!(pair_types, vec![PairType::Custom("Custom".to_string())]);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = FactoryHelper::init(&mut app, &owner);

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.factory.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(Addr::unchecked("owner"), helper.factory.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap_err();
    // new_owner is not an owner yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::DropOwnershipProposal {},
        &[],
    )
    .unwrap();

    // Try to claim ownership
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner again
    app.execute_contract(Addr::unchecked("owner"), helper.factory.clone(), &msg, &[])
        .unwrap();
    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.factory.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app.wrap().query_wasm_smart(&helper.factory, &msg).unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn test_create_permissioned_pair() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token2 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);

    let err = helper
        .create_pair(
            &mut app,
            &Addr::unchecked("random_stranger"),
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap();
}

#[test]
fn test_create_permissioned_pair_whitelist() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let random_stranger = Addr::unchecked("random_stranger");
    let whitelisted = Addr::unchecked("whitelisted");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let config = helper.query_config(&mut app).unwrap();
    // Find the pair config for "transmuter"
    let transmuter_config = config
        .pair_configs
        .iter()
        .find(|c| matches!(&c.pair_type, PairType::Custom(s) if s == "transmuter"))
        .unwrap()
        .clone();

    let token1 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token2 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);
    let token3 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenZ", None);

    let err = helper
        .create_pair(
            &mut app,
            &random_stranger,
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    // should also not yet be able to create a pair with whitelisted address
    let err = helper
        .create_pair(
            &mut app,
            &whitelisted,
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    let err = app
        .execute_contract(
            random_stranger.clone(),
            helper.factory.clone(),
            &ExecuteMsg::UpdatePairConfig {
                config: PairConfig {
                    whitelist: Some(vec![
                        Addr::unchecked("whitelisted").to_string(),
                        Addr::unchecked("whitelisted").to_string(),
                    ]),
                    ..transmuter_config.clone()
                },
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    // Add whitelist
    let err = app
        .execute_contract(
            owner.clone(),
            helper.factory.clone(),
            &ExecuteMsg::UpdatePairConfig {
                config: PairConfig {
                    whitelist: Some(vec![
                        Addr::unchecked("whitelisted").to_string(),
                        Addr::unchecked("whitelisted").to_string(),
                    ]),
                    ..transmuter_config.clone()
                },
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PairConfigDuplicateWhitelist {}
    );

    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::UpdatePairConfig {
            config: PairConfig {
                whitelist: Some(vec![Addr::unchecked("whitelisted").to_string()]),
                ..transmuter_config.clone()
            },
        },
        &[],
    )
    .unwrap();

    // stranger not allowed
    let err = helper
        .create_pair(
            &mut app,
            &Addr::unchecked("random_stranger"),
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    // owner still allowed
    helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Custom("transmuter".to_string()),
            [&token1, &token2],
            None,
        )
        .unwrap();

    // whitelisted address allowed
    helper
        .create_pair(
            &mut app,
            &whitelisted,
            PairType::Custom("transmuter".to_string()),
            [&token1, &token3],
            None,
        )
        .unwrap();
}

#[test]
fn tracker_config() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    // Should return an error since tracker config is not set
    let err = helper.query_tracker_config(&mut app).unwrap_err();

    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: Generic error: Tracker config is not set in the factory. It can't be provided")
    );

    // should return an error since the sender is not the owner
    let err = helper
        .update_tracker_config(&mut app, &Addr::unchecked("not_owner"), 64, None)
        .unwrap_err()
        .downcast::<ContractError>()
        .unwrap();

    assert_eq!(err, ContractError::Unauthorized {});

    // should return an error if trying to update code_id and token_factory_add is not provided

    let err = helper
        .update_tracker_config(&mut app, &owner, 64, None)
        .unwrap_err()
        .downcast::<ContractError>()
        .unwrap();

    assert_eq!(
        err,
        ContractError::Std(StdError::generic_err("token_factory_addr is required"))
    );

    // should success if the sender is the owner and the token_factory_addr is provided
    helper
        .update_tracker_config(&mut app, &owner, 64, Some("token_factory_addr".to_string()))
        .unwrap();

    // should return the tracker config
    let tracker_config = helper.query_tracker_config(&mut app).unwrap();
    assert_eq!(tracker_config.token_factory_addr, "token_factory_addr");
    assert_eq!(tracker_config.code_id, 64);

    // Query tracker config should work since the beggining if the tracker config is set when the contract is instantiated
    let init_msg = astroport::factory::InstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: 0,
            maker_fee_bps: 3333,
            total_fee_bps: 30u16,
            pair_type: PairType::Xyk {},
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
            whitelist: None,
        }],
        token_code_id: 0,
        generator_address: None,
        owner: owner.to_string(),
        whitelist_code_id: 0,
        coin_registry_address: "registry".to_string(),
        tracker_config: Some(TrackerConfig {
            code_id: 64,
            token_factory_addr: "token_factory_addr".to_string(),
        }),
    };

    let factory = app
        .instantiate_contract(3, owner.clone(), &init_msg, &[], "factory", None)
        .unwrap();

    let tracker_config = app
        .wrap()
        .query_wasm_smart::<astroport::factory::TrackerConfig>(
            factory.clone(),
            &astroport::factory::QueryMsg::TrackerConfig {},
        )
        .unwrap();

    assert_eq!(tracker_config.token_factory_addr, "token_factory_addr");
    assert_eq!(tracker_config.code_id, 64);
}
