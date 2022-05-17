use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, Addr};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PairConfig, PairType, QueryMsg,
};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cw20::MinterResponse;

use terra_multi_test::{AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock};

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

fn store_factory_code(app: &mut TerraApp) -> u64 {
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

fn store_pair_code(app: &mut TerraApp) -> u64 {
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

fn store_token_code(app: &mut TerraApp) -> u64 {
    let token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(token_contract)
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

    let owner = String::from("owner");

    let token_code_id = store_token_code(&mut app);
    let factory_instance =
        instantiate_contract(&mut app, &Addr::unchecked(owner.clone()), token_code_id);

    // Update config
    let fee_address = Some(String::from("fee"));
    let generator_address = Some(String::from("generator"));

    let msg = ExecuteMsg::UpdateConfig {
        token_code_id: Some(200u64),
        fee_address: fee_address.clone(),
        generator_address: generator_address.clone(),
        whitelist_code_id: None,
    };

    app.execute_contract(
        Addr::unchecked(owner.clone()),
        factory_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = QueryMsg::Config {};
    let config_res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&factory_instance, &msg)
        .unwrap();

    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!(
        fee_address.unwrap(),
        config_res.fee_address.unwrap().to_string()
    );
    assert_eq!(
        generator_address.unwrap(),
        config_res.generator_address.unwrap().to_string()
    );

    // Unauthorized err
    let msg = ExecuteMsg::UpdateConfig {
        token_code_id: None,
        fee_address: None,
        generator_address: None,
        whitelist_code_id: None,
    };

    let res = app
        .execute_contract(
            Addr::unchecked("invalid_owner"),
            factory_instance,
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(res.to_string(), "Unauthorized");
}

fn instantiate_contract(app: &mut TerraApp, owner: &Addr, token_code_id: u64) -> Addr {
    let pair_code_id = store_pair_code(app);
    let factory_code_id = store_factory_code(app);

    let pair_configs = vec![PairConfig {
        code_id: pair_code_id,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: false,
        is_generator_disabled: false,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    app.instantiate_contract(
        factory_code_id,
        owner.to_owned(),
        &msg,
        &[],
        "factory",
        None,
    )
    .unwrap()
}

fn instantiate_token(
    app: &mut TerraApp,
    token_code_id: u64,
    owner: &Addr,
    token_name: &str,
) -> Addr {
    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 18,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
    };

    app.instantiate_contract(
        token_code_id,
        owner.clone(),
        &init_msg,
        &[],
        token_name,
        None,
    )
    .unwrap()
}

fn create_pair(
    app: &mut TerraApp,
    owner: &Addr,
    factory: &Addr,
    token1: &Addr,
    token2: &Addr,
) -> Addr {
    let asset_infos = [
        AssetInfo::Token {
            contract_addr: token1.clone(),
        },
        AssetInfo::Token {
            contract_addr: token2.clone(),
        },
    ];

    let msg = ExecuteMsg::CreatePair {
        pair_type: PairType::Xyk {},
        asset_infos: asset_infos.clone(),
        init_params: None,
    };

    app.execute_contract(owner.clone(), factory.clone(), &msg, &[])
        .unwrap();

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            factory.clone(),
            &QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            },
        )
        .unwrap();

    res.contract_addr
}

#[test]
fn test_create_pair() {
    let mut app = mock_app();

    let owner = String::from("owner");

    let token_code_id = store_token_code(&mut app);

    let factory_instance =
        instantiate_contract(&mut app, &Addr::unchecked(owner.clone()), token_code_id);

    let owner_addr = Addr::unchecked(owner.clone());

    let token_name = "tokenX";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 18,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner_addr.to_string(),
            cap: None,
        }),
    };

    let token_instance0 = app
        .instantiate_contract(
            token_code_id,
            owner_addr.clone(),
            &init_msg,
            &[],
            token_name,
            None,
        )
        .unwrap();

    let token_name = "tokenY";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 18,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner_addr.to_string(),
            cap: None,
        }),
    };

    let token_instance1 = app
        .instantiate_contract(
            token_code_id,
            owner_addr.clone(),
            &init_msg,
            &[],
            token_name,
            None,
        )
        .unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: token_instance0.clone(),
        },
        AssetInfo::Token {
            contract_addr: token_instance1.clone(),
        },
    ];

    let msg = ExecuteMsg::CreatePair {
        pair_type: PairType::Xyk {},
        asset_infos: asset_infos.clone(),
        init_params: None,
    };

    let res = app
        .execute_contract(Addr::unchecked(owner), factory_instance.clone(), &msg, &[])
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", "contract #1-contract #2")
    );

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            factory_instance.clone(),
            &QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            },
        )
        .unwrap();

    // In multitest, contract names are counted in the order in which contracts are created
    assert_eq!("contract #0", factory_instance.to_string());
    assert_eq!("contract #3", res.contract_addr.to_string());
    assert_eq!("contract #4", res.liquidity_token.to_string());
}

#[test]
fn test_pair_migration() {
    let mut app = mock_app();

    let owner = String::from("owner");

    let token_code_id = store_token_code(&mut app);

    let factory_instance =
        instantiate_contract(&mut app, &Addr::unchecked(owner.clone()), token_code_id);

    let owner_addr = Addr::unchecked(owner.clone());

    let token_instance0 = instantiate_token(&mut app, token_code_id, &owner_addr, "tokenX");
    let token_instance1 = instantiate_token(&mut app, token_code_id, &owner_addr, "tokenY");
    let token_instance2 = instantiate_token(&mut app, token_code_id, &owner_addr, "tokenZ");

    // Create pairs in factory
    let pairs = [
        create_pair(
            &mut app,
            &owner_addr,
            &factory_instance,
            &token_instance0,
            &token_instance1,
        ),
        create_pair(
            &mut app,
            &owner_addr,
            &factory_instance,
            &token_instance0,
            &token_instance2,
        ),
    ];

    // Change contract ownership
    let new_owner = Addr::unchecked("new_owner");

    app.execute_contract(
        owner_addr.clone(),
        factory_instance.clone(),
        &ExecuteMsg::ProposeNewOwner {
            owner: new_owner.to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        new_owner.clone(),
        factory_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    let pair3 = create_pair(
        &mut app,
        &owner_addr,
        &factory_instance,
        &token_instance1,
        &token_instance2,
    );

    // Should panic due to pairs are not migrated.
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

        assert_eq!(res.to_string(), "Pair is not migrated to the new admin!");
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

    app.execute_contract(
        new_owner,
        factory_instance,
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
