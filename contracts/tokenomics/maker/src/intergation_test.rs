use crate::msg::{InitMsg, ExecuteMsg};
use astroport::asset::{AssetInfo, PairInfo, Asset};
use astroport::factory::{ConfigResponse, PairsResponse};
use astroport::token::InstantiateMsg;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse, AllAllowancesResponse, AllowanceInfo, Expiration};
use cw_multi_test::{App, ContractWrapper, Executor, BankKeeper};

fn mock_app() -> App {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();

    App::new(api, env.block, bank, MockStorage::new())
}


fn instantiate_contracts(router: &mut App, owner: Addr, staking: Addr) -> (Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
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
            None,
        )
        .unwrap();

    let factory_contract = Box::new(ContractWrapper::new(
        astroport_factory::contract::execute,
        astroport_factory::contract::instantiate,
        astroport_factory::contract::query,
    ));

    let factory_code_id = router.store_code(factory_contract);

    let msg = astroport::factory::InstantiateMsg {
        pair_code_ids: vec![5u64, 6u64, 12u64, 13u64, 23u64],
        token_code_id: 1u64,
        init_hook: None,
        fee_address: None,
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("FACTORY"),
            None,
        )
        .unwrap();

    let maker_contract = Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));
    let market_code_id = router.store_code(maker_contract);

    let msg = InitMsg {
        factory: factory_instance.clone(),
        staking,
        astro: astro_token_instance.clone(),
    };
    let maker_instance = router
        .instantiate_contract(market_code_id, owner, &msg, &[], String::from("MAKER"), None)
        .unwrap();
    (astro_token_instance, factory_instance, maker_instance)
}

fn instantiate_token(router: &mut App, owner: Addr, name: String, symbol: String) -> (u64, Addr) {
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
        marketing: None
    };

    let token_instance = router
        .instantiate_contract(token_code_id.clone(), owner.clone(), &msg, &[], symbol, None)
        .unwrap();
    (token_code_id, token_instance)
}

fn instantiate_pair(
    router: &mut App,
    owner: Addr,
    factory: Addr,
    token1: &str,
    token2: &str,
) -> (u64, Addr) {
    let token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let token_contract_code_id = router.store_code(token_contract);

    let pair_contract = Box::new(ContractWrapper::new(
        astroport_pair::contract::execute,
        astroport_pair::contract::instantiate,
        astroport_pair::contract::query,
    ));

    let pair_contract_code_id = router.store_code(pair_contract);

    let msg = astroport::pair::InstantiateMsg {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: token1.to_string(),
            },
            AssetInfo::NativeToken {
                denom: token2.to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        init_hook: None,
        factory_addr: factory,
    };

    let pair = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
            None,
        )
        .unwrap();
    (pair_contract_code_id, pair)
}

fn mint_some_token(
    router: &mut App,
    owner: Addr,
    token_instance: Addr,
    to: Addr,
    amount: Uint128,
) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.to_string(),
        amount,
    };
    let res = router
        .execute_contract(owner.clone(), token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to.to_string()));
    assert_eq!(
        res.events[1].attributes[3],
        attr("amount", amount)
    );
}

fn allowance_token(
    router: &mut App,
    owner: Addr,
    spender: Addr,
    token: Addr,
    amount: Uint128,
) {

    let msg = cw20::Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        amount,
        expires: None
    };
    let res = router
        .execute_contract(owner.clone(), token.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "increase_allowance"));
    assert_eq!(res.events[1].attributes[2], attr("owner", owner.to_string()));
    assert_eq!(res.events[1].attributes[3], attr("spender", spender.to_string()));
    assert_eq!(
        res.events[1].attributes[4],
        attr("amount", amount)
    );

}

#[test]
fn test() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let (_usdc_id, usdc_instance) = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    // mint 100 ASTRO for user
    mint_some_token(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        user.clone(),
        Uint128::from(100u128),
    );

    // mint 100 USDC for user
    mint_some_token(
        &mut router,
        owner.clone(),
        usdc_instance.clone(),
        user.clone(),
        Uint128::from(100u128),
    );

    let (pair_code_id, _pair_instance) = instantiate_pair(
        &mut router,
        owner.clone(),
        factory_instance.clone(),
        "astro",
        "usdc",
    );



    // let res: Result<ConfigResponse, _>  = router
    //     .wrap()
    //     .query(&QueryRequest::Wasm(WasmQuery::Smart {
    //         contract_addr: factory_instance.clone().to_string(),
    //         msg: to_binary(&astroport::factory::QueryMsg::Config {}).unwrap(),
    //     }));
    // assert_eq!(res.unwrap().pair_code_ids, vec![5u64, 6u64, 12u64, 13u64, 23u64]);



    let asset_infos = [
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: usdc_instance.clone(),
        },
    ];

    let msg = astroport::factory::ExecuteMsg::CreatePair {
        pair_code_id: pair_code_id.clone(),
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let res = router
        .execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    assert_eq!( res.events[1].attributes[1],attr("action", "create_pair"));
    //assert_eq!( res.events[1].attributes[2], attr("pair", format!("{}-{}", "astro", "usdc")));


    let pair_info: PairInfo  = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair { asset_infos: asset_infos.clone() }).unwrap(),
        })).unwrap();


    allowance_token( &mut router, user.clone(), pair_info.contract_addr.clone(), astro_token_instance.clone(), Uint128::from(100u128));
    allowance_token( &mut router, user.clone(), pair_info.contract_addr.clone(), usdc_instance.clone(), Uint128::from(100u128));

    let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset{
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100u128)
            },
            Asset{
                info: AssetInfo::Token {
                    contract_addr: usdc_instance.clone(),
                },
                amount: Uint128::from(100u128)
            }
        ],
        slippage_tolerance: None
    };
    let res = router
        .execute_contract(user.clone(), pair_info.contract_addr.clone(), &msg, &[])
        .unwrap();

    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );
    check_balance(&mut router, maker_instance.clone(), pair_info.liquidity_token.clone(), Uint128::from(10u64));
    check_balance(&mut router, maker_instance.clone(), astro_token_instance.clone(), Uint128::zero());
    check_balance(&mut router, staking.clone(), astro_token_instance.clone(), Uint128::zero());

    let msg = ExecuteMsg::Convert {
        token1: AssetInfo::Token { contract_addr: astro_token_instance.clone()},
        token2: AssetInfo::Token { contract_addr: usdc_instance.clone(),},
    };

    let res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
    //assert_eq!( res.events[1].attributes[1],attr("action", "create_pair"));
    check_balance(&mut router, maker_instance.clone(), pair_info.liquidity_token, Uint128::zero());
    check_balance(&mut router, maker_instance.clone(), astro_token_instance.clone(), Uint128::zero());
    check_balance(&mut router, staking.clone(), astro_token_instance.clone(), Uint128::from(18u128));
    check_balance(&mut router, maker_instance.clone(), usdc_instance.clone(), Uint128::zero());
    // let res: Result<AllAllowancesResponse, _>  = router
    //     .wrap()
    //     .query(&QueryRequest::Wasm(WasmQuery::Smart {
    //         contract_addr: usdc_instance.clone().to_string(),
    //         msg: to_binary(&cw20::Cw20QueryMsg::AllAllowances {
    //             owner: maker_instance.to_string(),
    //             start_after: None,
    //             limit: None
    //         }).unwrap(),
    //     }));
    // assert_eq!(res.unwrap().allowances, vec![AllowanceInfo{
    //     spender: pair_info.contract_addr.to_string(),
    //     allowance: Uint128::from(9u64),
    //     expires: Expiration::Never {},
    // }]);


}

fn check_balance(router: &mut App, user: Addr, token: Addr, expected_amount: Uint128) {
    let msg = Cw20QueryMsg::Balance {
        address: user.to_string(),
    };
    let res: Result<BalanceResponse, _> =
        router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token.to_string(),
            msg: to_binary(&msg).unwrap(),
        }));
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: expected_amount
        }
    );
}
