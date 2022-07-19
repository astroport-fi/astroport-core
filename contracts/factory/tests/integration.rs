use cosmwasm_std::{attr, Addr};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PairConfig, PairType, QueryMsg,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cw20::MinterResponse;
use cw_multi_test::{App, BasicApp, ContractWrapper, Executor};

type TerraApp = App;
fn mock_app() -> TerraApp {
    BasicApp::default()
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
    assert_eq!(res.root_cause().to_string(), "Unauthorized");
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

#[test]
fn create_pair() {
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
        marketing: None,
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
        marketing: None,
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
        attr("pair", "contract1-contract2")
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

    // in multitest, contract names are named in the order in which contracts are created.
    assert_eq!("contract0", factory_instance.to_string());
    assert_eq!("contract3", res.contract_addr.to_string());
    assert_eq!("contract4", res.liquidity_token.to_string());
}
