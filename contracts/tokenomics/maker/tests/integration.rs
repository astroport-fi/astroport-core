use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, Coin, QueryRequest, Uint128, Uint64, WasmQuery};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use astroport::factory::{PairConfig, PairType};
use astroport_maker::msg::{
    ExecuteMsg, InstantiateMsg, QueryBalancesResponse, QueryConfigResponse, QueryMsg,
};

fn mock_app() -> App {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let tmq = TerraMockQuerier::new(MockQuerier::new(&[]));

    App::new(api, env.block, bank, MockStorage::new(), tmq)
}

fn instantiate_contracts(
    router: &mut App,
    owner: Addr,
    staking: Addr,
    governance: &Addr,
    governance_percent: Uint64,
) -> (Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = TokenInstantiateMsg {
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

    let pair_contract = Box::new(ContractWrapper::new(
        astroport_pair::contract::execute,
        astroport_pair::contract::instantiate,
        astroport_pair::contract::query,
    ));

    let pair_code_id = router.store_code(pair_contract);

    let factory_contract = Box::new(ContractWrapper::new(
        astroport_factory::contract::execute,
        astroport_factory::contract::instantiate,
        astroport_factory::contract::query,
    ));

    let factory_code_id = router.store_code(factory_contract);
    let msg = astroport::factory::InstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 0,
            maker_fee_bps: 0,
        }],
        token_code_id: 1u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        generator_address: Addr::unchecked("generator"),
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
            astroport_maker::contract::execute,
            astroport_maker::contract::instantiate,
            astroport_maker::contract::query,
        )
        .with_reply(astroport_maker::contract::reply),
    );
    let market_code_id = router.store_code(maker_contract);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        staking_contract: staking.to_string(),
        governance_contract: Option::from(governance.to_string()),
        governance_percent: Option::from(governance_percent),
        astro_token_contract: astro_token_instance.to_string(),
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

    let msg = TokenInstantiateMsg {
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

    let balance = res.unwrap();

    assert_eq!(balance.balance, expected_amount);
}

fn create_pair(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    factory_instance: &Addr,
    assets: [Asset; 2],
) -> PairInfo {
    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                mint_some_token(
                    &mut router,
                    owner.clone(),
                    contract_addr.clone(),
                    user.clone(),
                    a.amount,
                );
            }

            _ => {}
        }
    }

    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];

    // Create pair in factory
    let res = router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Xyk {},
                asset_infos: asset_infos.clone(),
                init_hook: None,
            },
            &[],
        )
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

    // Get pair
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

    let mut funds = vec![];

    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                allowance_token(
                    &mut router,
                    user.clone(),
                    pair_info.contract_addr.clone(),
                    contract_addr.clone(),
                    a.amount.clone(),
                );
            }
            AssetInfo::NativeToken { denom } => {
                funds.push(Coin {
                    denom,
                    amount: a.amount,
                });
            }
        }
    }

    router.init_bank_balance(&user, funds.clone()).unwrap();

    router
        .execute_contract(
            user.clone(),
            pair_info.contract_addr.clone(),
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets,
                slippage_tolerance: None,
                auto_stack: None,
            },
            &funds,
        )
        .unwrap();

    pair_info
}

#[test]
fn collect_all() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance = Addr::unchecked("governance");
    let governance_percent = Uint64::new(10);

    let (astro_token_instance, factory_instance, maker_instance) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        &governance,
        governance_percent,
    );

    let msg = QueryMsg::Config {};
    let res: QueryConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &msg)
        .unwrap();
    assert_eq!(res.governance_percent, governance_percent);

    let governance_percent = Uint64::new(50);

    let msg = ExecuteMsg::SetConfig {
        governance_percent: Some(governance_percent),
        governance_contract: None,
        staking_contract: None,
    };

    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::Config {};
    let res: QueryConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &msg)
        .unwrap();
    assert_eq!(res.governance_percent, governance_percent);

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

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    // Mint all tokens for maker
    let mut pair_addresses = vec![];

    for t in vec![
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uusd_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uluna_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uusd_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uluna_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: usdc_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: luna_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: usdc_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: luna_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
    ] {
        let pair_info = create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
        );

        pair_addresses.push(pair_info.contract_addr);
    }

    // Mint all tokens for maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (luna_token_instance.clone(), 30u128),
    ] {
        let (token, amount) = t;
        mint_some_token(
            &mut router,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &mut router,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    router
        .init_bank_balance(
            &maker_instance,
            vec![
                Coin {
                    denom: uusd_asset.clone(),
                    amount: Uint128::new(100),
                },
                Coin {
                    denom: uluna_asset.clone(),
                    amount: Uint128::new(110),
                },
            ],
        )
        .unwrap();

    let expected_balances = vec![
        Asset {
            info: AssetInfo::NativeToken {
                denom: uusd_asset.clone(),
            },
            amount: Uint128::new(100),
        },
        Asset {
            info: AssetInfo::NativeToken {
                denom: uluna_asset.clone(),
            },
            amount: Uint128::new(110),
        },
        Asset {
            info: AssetInfo::Token {
                contract_addr: astro_token_instance.clone(),
            },
            amount: Uint128::new(10),
        },
        Asset {
            info: AssetInfo::Token {
                contract_addr: usdc_token_instance.clone(),
            },
            amount: Uint128::new(20),
        },
        Asset {
            info: AssetInfo::Token {
                contract_addr: luna_token_instance.clone(),
            },
            amount: Uint128::new(30),
        },
    ];

    let balances_resp: QueryBalancesResponse = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_binary(&QueryMsg::Balances {
                assets: expected_balances.iter().map(|a| a.info.clone()).collect(),
            })
            .unwrap(),
        }))
        .unwrap();

    for b in expected_balances {
        let found = balances_resp
            .balances
            .iter()
            .find(|n| n.info.equal(&b.info))
            .unwrap();

        assert_eq!(found, &b);
    }

    router
        .execute_contract(
            maker_instance.clone(),
            maker_instance.clone(),
            &ExecuteMsg::Collect { pair_addresses },
            &[],
        )
        .unwrap();

    for t in vec![
        (astro_token_instance.clone(), 270u128), // 10 astro + 20 usdc + 30 luna + 100 uusd + 110 uluna
        (usdc_token_instance.clone(), 0u128),
        (luna_token_instance.clone(), 0u128),
    ] {
        let (token, amount) = t;

        // Check maker balance
        check_balance(
            &mut router,
            maker_instance.clone(),
            token.clone(),
            Uint128::zero(),
        );

        // Check balances
        let amount = Uint128::new(amount);
        let governance_amount =
            amount.multiply_ratio(Uint128::from(governance_percent), Uint128::new(100));
        let staking_amount = amount - governance_amount;
        check_balance(
            &mut router,
            governance.clone(),
            token.clone(),
            governance_amount,
        );
        check_balance(&mut router, staking.clone(), token, staking_amount);
    }
}

#[test]
fn collect_err_no_swap_pair() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance = Addr::unchecked("governance");
    let governance_percent = Uint64::new(50);

    let (astro_token_instance, factory_instance, maker_instance) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        &governance,
        governance_percent,
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    let mut pair_addresses = vec![];

    // Mint all tokens for maker
    for t in vec![
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uusd_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uusd_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: uluna_asset.clone(),
                },
                amount: Uint128::from(100_000_u128),
            },
        ],
    ] {
        let pair_info = create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
        );

        pair_addresses.push(pair_info.contract_addr);
    }

    // Mint all tokens for maker
    for t in vec![(astro_token_instance.clone(), 10u128)] {
        let (token, amount) = t;
        mint_some_token(
            &mut router,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &mut router,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    router
        .init_bank_balance(
            &maker_instance,
            vec![
                Coin {
                    denom: uusd_asset,
                    amount: Uint128::new(20),
                },
                Coin {
                    denom: uluna_asset,
                    amount: Uint128::new(30),
                },
            ],
        )
        .unwrap();

    let msg = ExecuteMsg::Collect { pair_addresses };

    let e = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        e.to_string(),
        "Cannot swap uluna to Contract #0. Pair not found in factory",
    );
}
