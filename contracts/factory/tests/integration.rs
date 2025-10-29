use astroport_test::cw_multi_test::{AppBuilder, BankSudo, ContractWrapper, Executor};
use cosmwasm_std::{attr, coin, Addr};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, PairConfig, PairType,
    PairsResponse, QueryMsg,
};
use astroport_factory::error::ContractError;
use astroport_test::modules::stargate::{MockStargate, StargateApp as App};

use crate::factory_helper::{instantiate_token, FactoryHelper};

mod factory_helper;

fn mock_app() -> App {
    AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {})
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
        coin_registry_address: "coin_registry".to_string(),
        creation_fee: Some(coin(1_000000, "astro")),
    };

    let factory_instance = app
        .instantiate_contract(factory_code_id, owner.clone(), &msg, &[], "factory", None)
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
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token1, &token1],
            None,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Doubling assets in asset infos"
    );

    let res = helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token1, &token2],
            None,
            &[],
        )
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
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
            &[],
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
fn test_create_pair_with_fee() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token2 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);

    let creation_fee = coin(1_000000, "astro");
    let fee_receiver = "maker".to_string();

    // Update config
    helper
        .update_config(
            &mut app,
            &owner,
            None,
            Some(fee_receiver.clone()),
            None,
            None,
            Some(creation_fee.clone()),
        )
        .unwrap();

    let err = helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token1, &token2],
            None,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        ContractError::CreationFeeExpected {
            fee: creation_fee.to_string()
        },
        err.downcast().unwrap(),
    );

    app.sudo(
        BankSudo::Mint {
            to_address: owner.to_string(),
            amount: vec![creation_fee.clone()],
        }
        .into(),
    )
    .unwrap();

    let res = helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token1, &token2],
            None,
            &[creation_fee.clone()],
        )
        .unwrap();

    // Assert fee address received fees
    assert_eq!(
        app.wrap()
            .query_balance(fee_receiver, &creation_fee.denom)
            .unwrap()
            .amount,
        creation_fee.amount
    );

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
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
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
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
            &[],
        )
        .unwrap();
}

#[test]
fn test_indexed_queries() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token2 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);
    let token3 = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenZ", None);

    // Create several pools for the same pair of assets
    for pair_type in [
        PairType::Xyk {},
        PairType::Xyk {},
        PairType::Custom("yet_another_xyk".to_string()),
    ] {
        helper
            .create_pair(&mut app, &owner, pair_type, [&token1, &token2], None, &[])
            .unwrap();
    }

    helper
        .create_pair(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token1, &token3],
            None,
            &[],
        )
        .unwrap();

    // Query all pairs
    let pairs_resp: PairsResponse = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::Pairs {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(pairs_resp.pairs.len(), 4);

    let duplicated_asset_infos = vec![
        AssetInfo::cw20(token1.clone()),
        AssetInfo::cw20(token2.clone()),
    ];
    let pairs: Vec<PairInfo> = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::PairsByAssetInfos {
                asset_infos: duplicated_asset_infos.clone(),
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(pairs.len(), 3);

    for pair in pairs.iter() {
        assert_eq!(pair.asset_infos, duplicated_asset_infos);
    }

    let pair: PairInfo = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::PairByLpToken {
                lp_token: pairs[0].liquidity_token.clone(),
            },
        )
        .unwrap();
    assert_eq!(pair, pairs[0]);
}
