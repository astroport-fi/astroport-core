use cosmwasm_std::{attr, Addr, Coin};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PairConfig, PairType, QueryMsg,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cw20::MinterResponse;
use classic_test_tube::{self, TerraTestApp, Wasm, SigningAccount, Module, Account};

fn store_factory_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let factory_contract = std::fs::read("../../../artifacts/astroport_factory.wasm").unwrap();
    let contract = wasm.store_code(&factory_contract, None, owner).unwrap();
    contract.data.code_id
}

fn store_pair_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let pair_contract = std::fs::read("../../../artifacts/astroport_pair_stable.wasm").unwrap();
    let contract = wasm.store_code(&pair_contract, None, owner).unwrap();
    contract.data.code_id
}

fn store_token_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let astro_token_contract = std::fs::read("../../../artifacts/astroport_token.wasm").unwrap();
    let contract = wasm.store_code(&astro_token_contract, None, owner).unwrap();
    contract.data.code_id
}

#[test]
fn proper_initialization() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let owner = &app.init_account(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
    ).unwrap();

    let factory_code_id = store_factory_code(&wasm, owner);

    let pair_configs = vec![PairConfig {
        code_id: 321,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: None,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id: 123,
        fee_address: None,
        owner: owner.address(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    let factory_instance = wasm
        .instantiate(
            factory_code_id, 
            &msg, 
            Some(owner.address().as_str()), 
            Some("FACTORY"), 
            &[], 
            owner
        ).unwrap();

    let msg = QueryMsg::Config {};
    let config_res: ConfigResponse = wasm
        .query(&factory_instance.data.address, &msg)
        .unwrap();

    assert_eq!(123, config_res.token_code_id);
    assert_eq!(pair_configs, config_res.pair_configs);
    assert_eq!(owner.address(), config_res.owner);
}

#[test]
fn update_config() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let accs = &app.init_accounts(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],2
    ).unwrap();

    let owner = &accs[0];
    let unauthorized = &accs[1];

    let token_code_id = store_token_code(&wasm, owner);
    let factory_instance = instantiate_contract(&wasm, owner, token_code_id);

    // update config
    let fee_address = Some(String::from("fee"));
    let generator_address = Some(String::from("generator"));

    let msg = ExecuteMsg::UpdateConfig {
        token_code_id: Some(200u64),
        fee_address: fee_address.clone(),
        generator_address: generator_address.clone(),
        whitelist_code_id: None,
    };

    wasm.execute(factory_instance.as_str(), &msg,&[], owner).unwrap();

    let msg = QueryMsg::Config {};
    let config_res: ConfigResponse = wasm
        .query(factory_instance.as_str(), &msg)
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

    let res = wasm.execute(factory_instance.as_str(), &msg, &[], unauthorized).unwrap_err();
    assert_eq!(res.to_string(), "Unauthorized");
}

fn instantiate_contract(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount, token_code_id: u64) -> Addr {
    let pair_code_id = store_pair_code(wasm, owner);
    let factory_code_id = store_factory_code(wasm, owner);

    let pair_configs = vec![PairConfig {
        code_id: pair_code_id,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: None,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id,
        fee_address: None,
        owner: owner.address(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    Addr::unchecked(wasm.instantiate(
        factory_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some("FACTORY"), 
        &[], 
        owner
    ).unwrap().data.address)
}

#[test]
fn create_pair() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let owner = &app.init_account(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
    ).unwrap();

    let token_code_id = store_token_code(&wasm, owner);

    let factory_instance = instantiate_contract(&wasm, owner.clone(), token_code_id);

    let token_name = "tokenX";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 18,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.address(),
            cap: None,
        }),
        marketing: None,
    };

    let token_instance0 = wasm.instantiate(
        token_code_id, 
        &init_msg, 
        Some(owner.address().as_str()), 
        Some(token_name), 
        &[], 
        owner
    ).unwrap();

    let token_name = "tokenY";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 18,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.address(),
            cap: None,
        }),
        marketing: None,
    };

    let token_instance1 = wasm.instantiate(
        token_code_id, 
        &init_msg, 
        Some(owner.address().as_str()), 
        Some(token_name),
        &[], 
        owner
    ).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: Addr::unchecked(token_instance0.data.address),
        },
        AssetInfo::Token {
            contract_addr: Addr::unchecked(token_instance1.data.address),
        },
    ];

    let msg = ExecuteMsg::CreatePair {
        pair_type: PairType::Xyk {},
        asset_infos: asset_infos.clone(),
        init_params: None,
    };

    let res = wasm.execute(factory_instance.as_str(), &msg, &[], owner).unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", "contract #1-contract #2")
    );

    let res: PairInfo = wasm
        .query(
            factory_instance.as_str(),
            &QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            },
        )
        .unwrap();

    // in multitest, contract names are named in the order in which contracts are created.
    assert_eq!("contract #0", factory_instance.to_string());
    assert_eq!("contract #3", res.contract_addr.to_string());
    assert_eq!("contract #4", res.liquidity_token.to_string());
}
