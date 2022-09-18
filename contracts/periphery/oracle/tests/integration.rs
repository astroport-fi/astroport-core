use cosmwasm_std::{
    attr, to_binary, Addr, BlockInfo, Coin, Decimal, QueryRequest, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, BasicApp, ContractWrapper, Executor};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use astroport::factory::{PairConfig, PairType};

use astroport::oracle::QueryMsg::Consult;
use astroport::oracle::{ExecuteMsg, InstantiateMsg};
use astroport::pair::StablePoolParams;

type TerraApp = App;
fn mock_app() -> TerraApp {
    BasicApp::default()
}

fn instantiate_contracts(router: &mut TerraApp, owner: Addr) -> (Addr, Addr, u64) {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
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
        marketing: None,
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

    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );

    let pair_code_id = router.store_code(pair_contract);

    let pair_stable_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_stable::contract::execute,
            astroport_pair_stable::contract::instantiate,
            astroport_pair_stable::contract::query,
        )
        .with_reply_empty(astroport_pair_stable::contract::reply),
    );

    let pair_stable_code_id = router.store_code(pair_stable_contract);

    let factory_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    );

    let factory_code_id = router.store_code(factory_contract);
    let msg = astroport::factory::InstantiateMsg {
        pair_configs: vec![
            PairConfig {
                code_id: pair_code_id,
                pair_type: PairType::Xyk {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: false,
            },
            PairConfig {
                code_id: pair_stable_code_id,
                pair_type: PairType::Stable {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: false,
            },
        ],
        token_code_id: 1u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
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

    let oracle_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_oracle::contract::execute,
        astroport_oracle::contract::instantiate,
        astroport_oracle::contract::query,
    ));
    let oracle_code_id = router.store_code(oracle_contract);
    (astro_token_instance, factory_instance, oracle_code_id)
}

fn instantiate_token(router: &mut TerraApp, owner: Addr, name: String, symbol: String) -> Addr {
    let token_contract = Box::new(ContractWrapper::new_with_empty(
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
        marketing: None,
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

fn mint_some_token(
    router: &mut TerraApp,
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
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

fn allowance_token(
    router: &mut TerraApp,
    owner: Addr,
    spender: Addr,
    token: Addr,
    amount: Uint128,
) {
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

fn check_balance(router: &mut TerraApp, user: Addr, token: Addr, expected_amount: Uint128) {
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
    mut router: &mut TerraApp,
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
                init_params: None,
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

    // When dealing with native tokens transfer should happen before contract call, which cw-multitest doesn't support
    for fund in funds.clone() {
        // we cannot transfer empty coins amount
        if !fund.amount.is_zero() {
            router
                .send_tokens(owner.clone(), user.clone(), &[fund])
                .unwrap();
        }
    }

    router
        .execute_contract(
            user.clone(),
            pair_info.contract_addr.clone(),
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets,
                slippage_tolerance: None,
                auto_stake: None,
                receiver: None,
            },
            &funds,
        )
        .unwrap();

    pair_info
}

fn create_pair_stable(
    mut router: &mut TerraApp,
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
                pair_type: PairType::Stable {},
                asset_infos: asset_infos.clone(),
                init_params: Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
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

    // When dealing with native tokens transfer should happen before contract call, which cw-multitest doesn't support
    for fund in funds.clone() {
        // we cannot transfer empty coins amount
        if !fund.amount.is_zero() {
            router
                .send_tokens(owner.clone(), user.clone(), &[fund])
                .unwrap();
        }
    }

    router
        .execute_contract(
            user.clone(),
            pair_info.contract_addr.clone(),
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets,
                slippage_tolerance: None,
                auto_stake: None,
                receiver: None,
            },
            &funds,
        )
        .unwrap();

    pair_info
}

fn change_provide_liquidity(
    mut router: &mut TerraApp,
    owner: Addr,
    user: Addr,
    pair_contract: Addr,
    token1: Addr,
    token2: Addr,
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
    check_balance(&mut router, user.clone(), token1.clone(), amount1);
    check_balance(&mut router, user.clone(), token2.clone(), amount2);
    allowance_token(
        &mut router,
        user.clone(),
        pair_contract.clone(),
        token1.clone(),
        amount1,
    );
    allowance_token(
        &mut router,
        user.clone(),
        pair_contract.clone(),
        token2.clone(),
        amount2,
    );
    router
        .execute_contract(
            user,
            pair_contract,
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
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
                slippage_tolerance: Some(Decimal::percent(50)),
                auto_stake: None,
                receiver: None,
            },
            &vec![],
        )
        .unwrap();
}

pub fn next_day(block: &mut BlockInfo) {
    block.time = block.time.plus_seconds(86400);
    block.height += 17280;
}

#[test]
fn consult() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut router, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_u128),
            },
        ],
    );
    router.update_block(next_day);
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

    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(50_000_u128),
        Uint128::from(50_000_u128),
    );
    router.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = router
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.root_cause().to_string(), "Period not elapsed",);
    router.update_block(next_day);

    // Change pair liquidity
    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user,
        pair_info.contract_addr,
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(10_000_u128),
        Uint128::from(10_000_u128),
    );
    router.update_block(next_day);
    router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount) in [
        (astro_token_instance.clone(), Uint128::from(1000u128)),
        (usdc_token_instance.clone(), Uint128::from(100u128)),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount);
    }
}

#[test]
fn consult_pair_stable() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut router, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair_stable(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_000000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000000u128),
            },
        ],
    );
    router.update_block(next_day);
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

    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(500_000_000000u128),
        Uint128::from(500_000_000000u128),
    );
    router.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = router
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.root_cause().to_string(), "Period not elapsed",);
    router.update_block(next_day);

    // Change pair liquidity
    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user,
        pair_info.contract_addr,
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(100_000_000000u128),
        Uint128::from(100_000_000000u128),
    );
    router.update_block(next_day);
    router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount) in [
        (astro_token_instance.clone(), Uint128::from(1000u128)),
        (usdc_token_instance.clone(), Uint128::from(100u128)),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount);
    }
}

#[test]
fn consult2() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut router, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(1000_000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(1000_000_u128),
            },
        ],
    );
    router.update_block(next_day);
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

    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(1000_u128),
        Uint128::from(1000_u128),
    );
    router.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = router
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.root_cause().to_string(), "Period not elapsed",);
    router.update_block(next_day);
    router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    // Change pair liquidity
    for (amount1, amount2) in [
        (Uint128::from(1000_u128), Uint128::from(500_u128)),
        (Uint128::from(1000_u128), Uint128::from(500_u128)),
    ] {
        change_provide_liquidity(
            &mut router,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            astro_token_instance.clone(),
            usdc_token_instance.clone(),
            amount1,
            amount2,
        );
        router.update_block(next_day);
        router
            .execute_contract(
                owner.clone(),
                oracle_instance.clone(),
                &ExecuteMsg::Update {},
                &[],
            )
            .unwrap();
    }
    for (addr, amount, amount_exp) in [
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(999u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1000u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_exp);
    }

    // Change pair liquidity
    for (amount1, amount2) in [
        (Uint128::from(250_u128), Uint128::from(350_u128)),
        (Uint128::from(250_u128), Uint128::from(350_u128)),
    ] {
        change_provide_liquidity(
            &mut router,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            astro_token_instance.clone(),
            usdc_token_instance.clone(),
            amount1,
            amount2,
        );
        router.update_block(next_day);
        router
            .execute_contract(
                owner.clone(),
                oracle_instance.clone(),
                &ExecuteMsg::Update {},
                &[],
            )
            .unwrap();
    }
    for (addr, amount, amount_exp) in [
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(999u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1000u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_exp);
    }
}

#[test]
fn consult_zero_price() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");

    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut router, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
        ],
    );
    router.update_block(next_day);
    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = router
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.root_cause().to_string(), "Period not elapsed",);
    router.update_block(next_day);
    router
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount_in, amount_out) in [
        (
            astro_token_instance.clone(),
            Uint128::from(100u128),
            Uint128::from(100u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(100u128),
            Uint128::from(100u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount: amount_in,
        };
        let res: Uint128 = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_out);
    }
}
