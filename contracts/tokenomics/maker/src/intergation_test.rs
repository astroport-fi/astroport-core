use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{Addr, from_binary, QueryRequest, WasmQuery, to_binary, Event, attr, Uint128};
use cw_multi_test::{App, ContractWrapper, SimpleBank};
//use terraswap::staking::{ConfigResponse, InstantiateMsg as xInstatiateMsg, QueryMsg};
use terraswap::token::InstantiateMsg;
use cw20::{MinterResponse, Cw20QueryMsg, BalanceResponse};
use crate::msg::InitMsg;
use terraswap::asset::AssetInfo;
use terraswap::factory::ConfigResponse;

fn mock_app() -> App {
    let env = mock_env();
    let api = Box::new(MockApi::default());
    let bank = SimpleBank {};

    App::new(api, env.block, bank, || Box::new(MockStorage::new()))
}

fn instantiate_contracts(router: &mut App, owner: Addr, staking: Addr) -> (Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new(
        terraswap_token::contract::execute,
        terraswap_token::contract::instantiate,
        terraswap_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = InstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        init_hook: None,
    };

    let astro_token_instance = router
        .instantiate_contract(
            astro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO"),
        )
        .unwrap();

    let factory_contract = Box::new(
        ContractWrapper::new(
            terraswap_factory::contract::execute,
            terraswap_factory::contract::instantiate,
            terraswap_factory::contract::query,
        )
    );

    let factory_code_id = router.store_code(factory_contract);

    let msg = terraswap::factory::InstantiateMsg {
        pair_code_ids: vec![12u64, 13u64, 23u64],
        token_code_id: 1u64,
        init_hook: None,
        fee_address: None
    };

    let factory_instance = router.instantiate_contract(
        factory_code_id,
        owner.clone(),
        &msg,
        &[],
        String::from("FACTORY"),
    ).unwrap();

    let maker_contract = Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));
    let market_code_id = router.store_code(maker_contract);

    let msg = InitMsg {
        factory: factory_instance.clone(),
        staking,
        astro: astro_token_instance.clone()
    };
    let maker_instance = router
        .instantiate_contract(market_code_id, owner, &msg, &[], String::from("MAKER"))
        .unwrap();
    (
        astro_token_instance,
        factory_instance,
        maker_instance,
    )
}

fn instantiate_token( router: &mut App, owner: Addr, name: String, symbol: String ) -> Addr {
    let token_contract = Box::new(ContractWrapper::new(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));

    let token_code_id = router.store_code(token_contract);

    let msg = cw20_base::msg::InstantiateMsg {
        name,
        symbol: symbol.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
    };

    let token_instance = router
        .instantiate_contract(
            token_code_id,
            owner.clone(),
            &msg,
            &[],
            symbol,
        )
        .unwrap();
    token_instance
}

fn mint_some_astro(router: &mut App, owner: Addr, astro_token_instance: Addr, to: &str, amount:Uint128) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: String::from(to),
        amount,
    };
    let res = router
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(
        res.events,
        vec![Event {
            ty: String::from("wasm"),
            attributes: vec![
                attr("contract_address", "Contract #0"),
                attr("action", "mint"),
                attr("to", String::from(to)),
                attr("amount", amount),
            ],
        }]
    );
}


#[test]
fn test(){
    let mut router = mock_app();

    let owner = Addr::unchecked("owner");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());


    // mint 100 ASTRO for Alice
    mint_some_astro(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        "USER",
        Uint128::from(100u128)
    );
    let user_address = Addr::unchecked("USER");

    // check if Alice's ASTRO balance is 100
    let msg = Cw20QueryMsg::Balance {
        address: user_address.to_string(),
    };
    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: astro_token_instance.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
        .unwrap();
    assert_eq!(
        from_binary::<BalanceResponse>(&res).unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );
    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        String::from("USDC token"),
        String::from("USDC"),
    );

    let astro_assets = AssetInfo::Token {contract_addr: astro_token_instance.clone()};
    let usdc_assets = AssetInfo::Token {contract_addr:usdc_token_instance};


    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&terraswap::factory::QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();

    let config = from_binary::<ConfigResponse>(&res).unwrap();
    assert_eq!(
        config.pair_code_ids,
        vec![12u64, 13u64, 23u64]
    );

    let msg = terraswap::factory::ExecuteMsg::CreatePair {
        pair_code_id: 12u64,
        asset_infos: [astro_assets.clone(), usdc_assets.clone()],
        init_hook: None,
    };

    let res = router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &msg,
            &[]
        ).unwrap();

    assert_eq!(
        res.events,
        vec![Event {
            ty: String::from("wasm"),
            attributes: vec![
                attr("action", "create_pair"),
                attr("pair", format!("{}-{}", astro_assets.clone(), usdc_assets.clone())),
            ]
        }]
    )


}
