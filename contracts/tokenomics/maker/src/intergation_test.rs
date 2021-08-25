use cosmwasm_std::{Addr, attr, Coin, Decimal, QueryRequest, to_binary, Uint128, WasmQuery};
use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
//use cw_multi_test::{App, BankKeeper, ContractWrapper, Executor};
use terra_multi_test::{App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg;

use crate::msg::{ExecuteMsg, InitMsg};

// pub use terra_mocks::TerraMockQuerier;

fn mock_app() -> App {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let mut tmq = TerraMockQuerier::new(MockQuerier::new(&[]));

    tmq.with_market(&[
        ("uusd", "astro", Decimal::percent(100)),
        ("luna", "astro", Decimal::percent(100)),
        ("uusd", "luna", Decimal::percent(100)),
    ]);

    App::new(api, env.block, bank, MockStorage::new(), tmq)
    //App::new(api, env.block, bank, MockStorage::new())
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
        pair_code_ids: vec![
            5u64, 6u64, 7u64, 8u64, 9u64, 10u64, 11u64, 12u64, 13u64, 23u64,
        ],
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

    let maker_contract = Box::new(
        ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        )
            .with_reply(crate::contract::reply),
    );
    let market_code_id = router.store_code(maker_contract);

    let msg = InitMsg {
        factory: factory_instance.clone(),
        staking,
        astro: astro_token_instance.clone(),
    };
    let maker_instance = router
        .instantiate_contract(
            market_code_id,
            owner,
            &msg,
            &[],
            String::from("MAKER"),
            None,
        )
        .unwrap();
    (astro_token_instance, factory_instance, maker_instance)
}

fn instantiate_token(router: &mut App, owner: Addr, name: String, symbol: String) -> Addr {
    let token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let token_code_id = router.store_code(token_contract);

    let msg = InstantiateMsg {
        name,
        symbol: symbol.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        //marketing: None,
        init_hook: None,
    };

    let token_instance = router
        .instantiate_contract(
            token_code_id.clone(),
            owner.clone(),
            &msg,
            &[],
            symbol,
            None,
        )
        .unwrap();
    token_instance
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

fn mint_some_token(router: &mut App, owner: Addr, token_instance: Addr, to: Addr, amount: Uint128) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.to_string(),
        amount,
    };
    let res = router
        .execute_contract(owner.clone(), token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to.to_string()));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

fn transfer_token(router: &mut App, owner: Addr, token_instance: Addr, to: Addr, amount: Uint128) {
    let msg = cw20::Cw20ExecuteMsg::Transfer {
        recipient: to.to_string(),
        amount,
    };
    let res = router
        .execute_contract(owner.clone(), token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "transfer"));
    assert_eq!(res.events[1].attributes[2], attr("from", owner.to_string()));
    assert_eq!(res.events[1].attributes[3], attr("to", to.to_string()));
    assert_eq!(res.events[1].attributes[4], attr("amount", amount));
}

fn allowance_token(router: &mut App, owner: Addr, spender: Addr, token: Addr, amount: Uint128) {
    let msg = cw20::Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        amount,
        expires: None,
    };
    let res = router
        .execute_contract(owner.clone(), token.clone(), &msg, &[])
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "increase_allowance")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("owner", owner.to_string())
    );
    assert_eq!(
        res.events[1].attributes[3],
        attr("spender", spender.to_string())
    );
    assert_eq!(res.events[1].attributes[4], attr("amount", amount));
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

fn create_pair(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    factory_instance: &Addr,
    token1: &Addr,
    token2: &Addr,
    amount1: Uint128,
    amount2: Uint128,
    name1: &str,
    name2: &str,
) -> PairInfo {
    mint_some_token(
        &mut router,
        owner.clone(),
        token1.clone(),
        user.clone(),
        amount1,
    );

    mint_some_token(
        &mut router,
        owner.clone(),
        token2.clone(),
        user.clone(),
        amount2,
    );

    let (pair_code_id, _pair_instance) = instantiate_pair(
        &mut router,
        owner.clone(),
        factory_instance.clone(),
        name1,
        name2,
    );
    let asset_infos = [
        AssetInfo::Token {
            contract_addr: token1.clone(),
        },
        AssetInfo::Token {
            contract_addr: token2.clone(),
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

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr(
            "pair",
            format!(
                "{}-{}",
                asset_infos[0].to_string(),
                asset_infos[1].to_string()
            ),
        )
    );

    let pair_info: PairInfo = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            })
                .unwrap(),
        }))
        .unwrap();

    allowance_token(
        &mut router,
        user.clone(),
        pair_info.contract_addr.clone(),
        token1.clone(),
        amount1.clone(),
    );
    allowance_token(
        &mut router,
        user.clone(),
        pair_info.contract_addr.clone(),
        token2.clone(),
        amount2.clone(),
    );

    let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token1.clone(),
                },
                amount: amount1,
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token2.clone(),
                },
                amount: amount2,
            },
        ],
        slippage_tolerance: None,
    };
    router
        .execute_contract(user.clone(), pair_info.contract_addr.clone(), &msg, &[])
        .unwrap();
    pair_info
}

fn add_liquidity(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    pair_instance: &Addr,
    token1: &Addr,
    token2: &Addr,
    amount1: Uint128,
    amount2: Uint128,
) {
    mint_some_token(
        &mut router,
        owner.clone(),
        token1.clone(),
        user.clone(),
        amount1,
    );

    mint_some_token(
        &mut router,
        owner.clone(),
        token2.clone(),
        user.clone(),
        amount2,
    );

    allowance_token(
        &mut router,
        user.clone(),
        pair_instance.clone(),
        token1.clone(),
        amount1.clone(),
    );
    allowance_token(
        &mut router,
        user.clone(),
        pair_instance.clone(),
        token2.clone(),
        amount2.clone(),
    );

    let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token1.clone(),
                },
                amount: amount1,
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token2.clone(),
                },
                amount: amount2,
            },
        ],
        slippage_tolerance: None,
    };
    router
        .execute_contract(user.clone(), pair_instance.clone(), &msg, &[])
        .unwrap();
}

#[test]
fn convert_token_astro_token_usdc() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &astro_token_instance,
        &usdc_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "astro",
        "usdc",
    );
    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    let msg = ExecuteMsg::Convert {
        token1: AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        token2: AssetInfo::Token {
            contract_addr: usdc_instance.clone(),
        },
    };
    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token,
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(18u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn convert_token_pair_not_exist() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, _factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let msg = ExecuteMsg::Convert {
        token2: AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        token1: AssetInfo::Token {
            contract_addr: usdc_instance.clone(),
        },
    };
    let res = router.execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[]);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(msg) => assert_eq!(
            msg.to_string(),
            "Generic error: Querier contract error: astroport::asset::PairInfo not found"
                .to_string()
        ),
    }
}

#[test]
fn convert_token_astro_token_usdc_2() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &astro_token_instance,
        &usdc_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "astro",
        "usdc",
    );
    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    let msg = ExecuteMsg::Convert {
        token2: AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        token1: AssetInfo::Token {
            contract_addr: usdc_instance.clone(),
        },
    };
    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token,
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(18u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
//#[ignore]
fn convert_token_astro_native_token_uusd() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    // mint 100 ASTRO for user
    mint_some_token(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        user.clone(),
        Uint128::from(100u128),
    );

    router
        .init_bank_balance(
            &user,
            vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(200u128),
            }],
        )
        .unwrap();

    let (pair_code_id, _pair_instance) = instantiate_pair(
        &mut router,
        owner.clone(),
        factory_instance.clone(),
        "astro",
        "uusd",
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        AssetInfo::NativeToken {
            denom: "uusd".to_string(),
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

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr(
            "pair",
            format!(
                "{}-{}",
                asset_infos[0].to_string(),
                asset_infos[1].to_string()
            ),
        )
    );

    let pair_info: PairInfo = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            })
                .unwrap(),
        }))
        .unwrap();

    allowance_token(
        &mut router,
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        Uint128::from(100u128),
    );

    let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100u128),
            },
        ],
        slippage_tolerance: None,
    };
    let _res = router
        .execute_contract(
            user.clone(),
            pair_info.contract_addr.clone(),
            &msg,
            &[Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(100u128),
            }],
        )
        .unwrap();

    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    let msg = ExecuteMsg::Convert {
        token1: AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        token2: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
    };
    // When dealing with native tokens transfer should happen before contract call, which cw-multitest doesn't support
    router
        .init_bank_balance(
            &maker_instance.clone(),
            vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(9u128),
            }],
        )
        .unwrap();
    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let bal = router
        .wrap()
        .query_all_balances(maker_instance.clone())
        .unwrap();
    assert_eq!(bal, vec![]);

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token,
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::from(Uint128::zero()),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(18u128),
    );
}

#[test]
fn convert_token_luna_token_astro() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let luna_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Luna token".to_string(),
        "LUNA".to_string(),
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &luna_token_instance,
        &astro_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "luna",
        "astro",
    );
    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    let msg = ExecuteMsg::Convert {
        token1: AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        token2: AssetInfo::Token {
            contract_addr: luna_token_instance.clone(),
        },
    };

    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token,
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(18u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        luna_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn convert_token_usdc_token_luna() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let luna_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Luna token".to_string(),
        "LUNA".to_string(),
    );
    let _pair_usdc_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &astro_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "usdc",
        "astro",
    );
    let _pair_luna_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &luna_token_instance,
        &astro_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "luna",
        "astro",
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &luna_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "usdc",
        "luna",
    );
    mint_some_token(
        &mut router,
        pair_info.contract_addr.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        Uint128::from(10u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    let msg = ExecuteMsg::Convert {
        token1: AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        token2: AssetInfo::Token {
            contract_addr: luna_token_instance.clone(),
        },
    };

    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token,
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(18u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn convert_multiple() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let luna_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Luna token".to_string(),
        "LUNA".to_string(),
    );

    let pair_usdc_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &astro_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "usdc",
        "astro",
    );
    let pair_luna_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &luna_token_instance,
        &astro_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "luna",
        "astro",
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &luna_token_instance,
        Uint128::from(100u128),
        Uint128::from(100u128),
        "usdc",
        "luna",
    );

    add_liquidity(
        &mut router,
        owner.clone(),
        maker_instance.clone(),
        &pair_usdc_astro.contract_addr,
        &usdc_token_instance,
        &astro_token_instance,
        Uint128::from(10u128),
        Uint128::from(10u128),
    );
    add_liquidity(
        &mut router,
        owner.clone(),
        maker_instance.clone(),
        &pair_luna_astro.contract_addr,
        &luna_token_instance,
        &astro_token_instance,
        Uint128::from(10u128),
        Uint128::from(10u128),
    );
    add_liquidity(
        &mut router,
        owner.clone(),
        maker_instance.clone(),
        &pair_info.contract_addr,
        &usdc_token_instance,
        &luna_token_instance,
        Uint128::from(10u128),
        Uint128::from(10u128),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_luna_astro.liquidity_token.clone(),
        Uint128::from(10u64),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::from(10u64),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    let msg = ExecuteMsg::ConvertMultiple {
        token1: vec![
            AssetInfo::Token {
                contract_addr: usdc_token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: usdc_token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: luna_token_instance.clone(),
            },
        ],

        token2: vec![
            AssetInfo::Token {
                contract_addr: luna_token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: astro_token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: astro_token_instance.clone(),
            },
        ],
    };

    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_luna_astro.liquidity_token.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::zero(),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(52u128),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        luna_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn convert_multiple2() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let luna_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Luna token".to_string(),
        "LUNA".to_string(),
    );

    let liquidity_amount = Uint128::from(1000_000_000_000_000_000u128);

    let pair_usdc_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &astro_token_instance,
        liquidity_amount,
        liquidity_amount,
        "usdc",
        "astro",
    );
    let pair_luna_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &luna_token_instance,
        &astro_token_instance,
        liquidity_amount,
        liquidity_amount,
        "luna",
        "astro",
    );
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &luna_token_instance,
        liquidity_amount,
        liquidity_amount,
        "usdc",
        "luna",
    );

    let amount_pair_usdc_astro = Uint128::from(10u128);
    let amount_pair_luna_astro = Uint128::from(10u128);
    let amount_pair_usdc_luna = Uint128::from(10u128);

    transfer_token(
        &mut router,
        user.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_usdc_astro.clone(),
    );
    transfer_token(
        &mut router,
        user.clone(),
        pair_luna_astro.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_luna_astro.clone(),
    );
    transfer_token(
        &mut router,
        user.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_usdc_luna.clone(),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        amount_pair_usdc_astro.clone(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_luna_astro.liquidity_token.clone(),
        amount_pair_luna_astro.clone(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        amount_pair_usdc_luna.clone(),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    //t1-a, t2-a, t1-t2 = 5987992029989 | 59
    //t1-t2, t2-a, t1-a = 5987992029988 | 58
    //t2-a, t1-t2, t1-a = 5987992029990 | 60

    let msg = ExecuteMsg::ConvertMultiple {
        token1: vec![
            AssetInfo::Token { //t1
                contract_addr: usdc_token_instance.clone(),
            },


            AssetInfo::Token { //t2
                contract_addr: luna_token_instance.clone(),
            },




            AssetInfo::Token { //t1
                contract_addr: usdc_token_instance.clone(),
            },
        ],
        token2: vec![
            AssetInfo::Token { //t2
                contract_addr: luna_token_instance.clone(),
            },

            AssetInfo::Token { //a
                contract_addr: astro_token_instance.clone(),
            },


            AssetInfo::Token { //a
                contract_addr: astro_token_instance.clone(),
            },


        ],
    };

    let _res = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_luna_astro.liquidity_token.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        pair_info.liquidity_token.clone(),
        Uint128::zero(),
    );

    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::from(58u128),
    );
    //TODO ???
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );
    check_balance(
        &mut router,
        maker_instance.clone(),
        luna_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn try_calc() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");

    let (astro_token_instance, factory_instance, maker_instance) =
        instantiate_contracts(&mut router, owner.clone(), staking.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );
    let luna_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Luna token".to_string(),
        "LUNA".to_string(),
    );

    let liquidity_amount0 = Uint128::from(1000u128);
    let liquidity_amount1 = Uint128::from(1000u128);

    //t1-a
    let pair_usdc_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &astro_token_instance,
        liquidity_amount0,
        liquidity_amount1,
        "usdc",
        "astro",
    );
    //t2-a
    let pair_luna_astro = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &luna_token_instance,
        &astro_token_instance,
        liquidity_amount0,
        liquidity_amount1,
        "luna",
        "astro",
    );
    //t1-t2
    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        &usdc_token_instance,
        &luna_token_instance,
        liquidity_amount0,
        liquidity_amount1,
        "usdc",
        "luna",
    );

    let amount_pair_usdc_astro = Uint128::from(10u128);
    let amount_pair_luna_astro = Uint128::from(10u128);
    let amount_pair_usdc_luna = Uint128::from(10u128);

    transfer_token(
        &mut router,
        user.clone(),
        pair_usdc_astro.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_usdc_astro.clone(),
    );
    transfer_token(
        &mut router,
        user.clone(),
        pair_luna_astro.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_luna_astro.clone(),
    );
    transfer_token(
        &mut router,
        user.clone(),
        pair_info.liquidity_token.clone(),
        maker_instance.clone(),
        amount_pair_usdc_luna.clone(),
    );

    let msg_t1_t2 = ExecuteMsg::Convert {
        token1: AssetInfo::Token {contract_addr: usdc_token_instance.clone()},
        token2: AssetInfo::Token { contract_addr: luna_token_instance.clone() },
    };
    let msg_t1_a = ExecuteMsg::Convert {
        token1: AssetInfo::Token { contract_addr: usdc_token_instance.clone() },
        token2: AssetInfo::Token { contract_addr: astro_token_instance.clone() },
    };
    let msg_t2_a = ExecuteMsg::Convert {
        token1: AssetInfo::Token { contract_addr: luna_token_instance.clone() },
        token2: AssetInfo::Token { contract_addr: astro_token_instance.clone() },
    };

    router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg_t1_t2, &[])
        .unwrap();
    balance_info(&mut router,staking.clone(),astro_token_instance.clone(), pair_info.clone(), pair_usdc_astro.clone(), pair_luna_astro.clone());
    router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg_t1_a, &[])
        .unwrap();
    balance_info(&mut router,staking.clone(),astro_token_instance.clone(), pair_info.clone(), pair_usdc_astro.clone(), pair_luna_astro.clone());
    router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg_t2_a, &[])
        .unwrap();
    balance_info(&mut router,staking.clone(),astro_token_instance.clone(), pair_info.clone(), pair_usdc_astro.clone(), pair_luna_astro.clone());
}

fn balance_info(router: &mut App, staking: Addr, astro_token: Addr, t1_t2: PairInfo, t1_a: PairInfo, t2_a:PairInfo) {
    let msg = Cw20QueryMsg::Balance {
        address: staking.to_string(),
    };
    let res: Result<BalanceResponse, _> =
        router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&msg).unwrap(),
        }));
    let staking_bal = res.unwrap().balance;
    let res: Result<BalanceResponse, _> =
        router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20QueryMsg::Balance {
                address: t1_t2.liquidity_token.to_string(),
            }).unwrap(),
        }));
    let lp_token_t1_t2_bal = res.unwrap().balance;



    println!("balance for staking contract: {}",  staking_bal.to_string());
}
