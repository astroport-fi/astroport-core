mod factory_helper;

use cosmwasm_std::{attr, Addr};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, PairConfig, PairType, QueryMsg,
};

use crate::factory_helper::{instantiate_token, FactoryHelper};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use cw_multi_test::{App, ContractWrapper, Executor};

fn mock_app() -> App {
    App::default()
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
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id: 123,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
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
    assert_eq!("contract5", res.liquidity_token.to_string());

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
fn test_pair_migration() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token_instance0 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token_instance1 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);
    let token_instance2 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenZ", None);

    // Create pairs in factory
    let pairs = [
        helper
            .create_pair_with_addr(
                &mut app,
                &owner,
                PairType::Xyk {},
                [&token_instance0, &token_instance1],
                None,
            )
            .unwrap(),
        helper
            .create_pair_with_addr(
                &mut app,
                &owner,
                PairType::Xyk {},
                [&token_instance0, &token_instance2],
                None,
            )
            .unwrap(),
    ];

    // Change contract ownership
    let new_owner = Addr::unchecked("new_owner");

    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::ProposeNewOwner {
            owner: new_owner.to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        new_owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    let pair3 = helper
        .create_pair_with_addr(
            &mut app,
            &owner,
            PairType::Xyk {},
            [&token_instance1, &token_instance2],
            None,
        )
        .unwrap();

    // Should panic due to pairs are not migrated.
    for pair in pairs.clone() {
        let res = app
            .execute_contract(
                new_owner.clone(),
                pair,
                &PairExecuteMsg::UpdateConfig {
                    params: Default::default(),
                },
                &[],
            )
            .unwrap_err();

        assert_eq!(
            res.root_cause().to_string(),
            "Pair is not migrated to the new admin!"
        );
    }

    // Pair is created after admin migration
    let res = app
        .execute_contract(
            Addr::unchecked("user1"),
            pair3,
            &PairExecuteMsg::UpdateConfig {
                params: Default::default(),
            },
            &[],
        )
        .unwrap_err();

    assert_ne!(res.to_string(), "Pair is not migrated to the new admin");

    let pairs_res: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::PairsToMigrate {})
        .unwrap();
    assert_eq!(&pairs_res, &pairs);

    // Factory owner was changed to new owner
    let err = app
        .execute_contract(
            owner,
            helper.factory.clone(),
            &ExecuteMsg::MarkAsMigrated {
                pairs: Vec::from(pairs.clone().map(String::from)),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    app.execute_contract(
        new_owner,
        helper.factory.clone(),
        &ExecuteMsg::MarkAsMigrated {
            pairs: Vec::from(pairs.clone().map(String::from)),
        },
        &[],
    )
    .unwrap();

    for pair in pairs.clone() {
        let res = app
            .execute_contract(
                Addr::unchecked("user1"),
                pair,
                &PairExecuteMsg::UpdateConfig {
                    params: Default::default(),
                },
                &[],
            )
            .unwrap_err();

        assert_ne!(res.to_string(), "Pair is not migrated to the new admin!");
    }
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
