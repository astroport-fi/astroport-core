use anyhow::Result;
use cosmwasm_std::{
    attr, to_binary, Addr, BlockInfo, Coin, Decimal, QueryRequest, StdResult, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, AppResponse, ContractWrapper, Executor};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use astroport::factory::{PairConfig, PairType};

use astroport::oracle::QueryMsg::Consult;
use astroport::oracle::{ExecuteMsg, InstantiateMsg};
use astroport::pair::StablePoolParams;

const OWNER: &str = "owner";

fn mock_app(owner: Option<Addr>, coins: Option<Vec<Coin>>) -> App {
    if owner.is_some() && coins.is_some() {
        App::new(|router, _, storage| {
            // initialization moved to App construction
            router
                .bank
                .init_balance(storage, &owner.unwrap(), coins.unwrap())
                .unwrap()
        })
    } else {
        App::default()
    }
}

fn store_coin_registry_code(app: &mut App) -> u64 {
    let coin_registry_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ));

    app.store_code(coin_registry_contract)
}

fn instantiate_coin_registry(mut app: &mut App, coins: Option<Vec<(String, u8)>>) -> Addr {
    let coin_registry_id = store_coin_registry_code(&mut app);
    let coin_registry_address = app
        .instantiate_contract(
            coin_registry_id,
            Addr::unchecked(OWNER),
            &astroport::native_coin_registry::InstantiateMsg {
                owner: OWNER.to_string(),
            },
            &[],
            "Coin registry",
            None,
        )
        .unwrap();

    if let Some(coins) = coins {
        app.execute_contract(
            Addr::unchecked(OWNER),
            coin_registry_address.clone(),
            &astroport::native_coin_registry::ExecuteMsg::Add {
                native_coins: coins,
            },
            &[],
        )
        .unwrap();
    }

    coin_registry_address
}

fn instantiate_contracts(mut router: &mut App, owner: Addr) -> (Addr, Addr, u64) {
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

    let coin_registry_address = instantiate_coin_registry(
        &mut router,
        Some(vec![("uluna".to_string(), 6), ("cny".to_string(), 6)]),
    );

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
        coin_registry_address: coin_registry_address.to_string(),
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

fn instantiate_token(router: &mut App, owner: Addr, name: String, symbol: String) -> Addr {
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

fn provide_liquidity(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    pair_info: &PairInfo,
    assets: Vec<Asset>,
) -> Result<AppResponse> {
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

    for fund in funds.clone() {
        // we cannot transfer empty coins amount
        if !fund.amount.is_zero() {
            router
                .send_tokens(owner.clone(), user.clone(), &[fund])
                .unwrap();
        }
    }

    router.execute_contract(
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
}

fn create_pair(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    factory_instance: &Addr,
    assets: Vec<Asset>,
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

    let asset_infos = vec![assets[0].info.clone(), assets[1].info.clone()];

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

    pair_info
}

fn create_pair_stable(
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    factory_instance: &Addr,
    assets: Vec<Asset>,
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

    let asset_infos: Vec<AssetInfo> = assets.iter().cloned().map(|a| a.info).collect();

    // Create pair in factory
    let res = router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Stable {},
                asset_infos: asset_infos.clone(),
                init_params: Some(
                    to_binary(&StablePoolParams {
                        amp: 100,
                        owner: None,
                    })
                    .unwrap(),
                ),
            },
            &[],
        )
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", asset_infos.iter().join("-"),)
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
    mut router: &mut App,
    owner: Addr,
    user: Addr,
    pair_contract: Addr,
    assets: Vec<(Addr, Uint128)>,
) {
    for (token, amount) in assets.clone() {
        mint_some_token(
            &mut router,
            owner.clone(),
            token.clone(),
            user.clone(),
            amount,
        );
        check_balance(&mut router, user.clone(), token.clone(), amount);
        allowance_token(
            &mut router,
            user.clone(),
            pair_contract.clone(),
            token.clone(),
            amount,
        );
    }

    let assets: Vec<Asset> = assets
        .iter()
        .cloned()
        .map(|(token, amount)| Asset {
            info: AssetInfo::Token {
                contract_addr: token,
            },
            amount,
        })
        .collect();

    router
        .execute_contract(
            user,
            pair_contract,
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets,
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
    let mut router = mock_app(None, None);
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

    let asset_infos = vec![
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];

    let assets = vec![
        Asset {
            info: asset_infos[0].clone(),
            amount: Uint128::from(100_000_u128),
        },
        Asset {
            info: asset_infos[1].clone(),
            amount: Uint128::from(100_000_u128),
        },
    ];

    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        assets.clone(),
    );
    provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        assets.clone(),
    )
    .unwrap();

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
        vec![
            (astro_token_instance.clone(), Uint128::from(50_000_u128)),
            (usdc_token_instance.clone(), Uint128::from(50_000_u128)),
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

    // Change pair liquidity
    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user,
        pair_info.contract_addr,
        vec![
            (astro_token_instance.clone(), Uint128::from(10_000_u128)),
            (usdc_token_instance.clone(), Uint128::from(10_000_u128)),
        ],
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
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res[0].1, amount);
    }
}

#[test]
fn consult_pair_stable() {
    let mut router = mock_app(None, None);
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

    let asset_infos = vec![
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
        vec![
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
        vec![
            (
                astro_token_instance.clone(),
                Uint128::from(500_000_000000u128),
            ),
            (
                usdc_token_instance.clone(),
                Uint128::from(500_000_000000u128),
            ),
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

    // Change pair liquidity
    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user,
        pair_info.contract_addr,
        vec![
            (
                astro_token_instance.clone(),
                Uint128::from(100_000_000000u128),
            ),
            (
                usdc_token_instance.clone(),
                Uint128::from(100_000_000000u128),
            ),
        ],
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
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res[0].1, amount);
    }
}

#[test]
fn consult2() {
    let mut router = mock_app(None, None);
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

    let asset_infos = vec![
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];

    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(2000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(2000_u128),
            },
        ],
    );

    // try to provide less than 1000
    let err = provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_u128),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        "Initial liquidity must be more than 1000",
        err.root_cause().to_string()
    );

    // try to provide MINIMUM_LIQUIDITY_AMOUNT
    let err = provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(1000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(1000_u128),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        "Initial liquidity must be more than 1000",
        err.root_cause().to_string()
    );

    // try to provide more then MINIMUM_LIQUIDITY_AMOUNT
    provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(2000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(2000_u128),
            },
        ],
    )
    .unwrap();

    router.update_block(next_day);

    change_provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        vec![
            (astro_token_instance.clone(), Uint128::from(1000_u128)),
            (usdc_token_instance.clone(), Uint128::from(1000_u128)),
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
    assert_eq!(e.root_cause().to_string(), "Period not elapsed");
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
            vec![
                (astro_token_instance.clone(), amount1),
                (usdc_token_instance.clone(), amount2),
            ],
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
            Uint128::from(800u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1250u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res[0].1, amount_exp);
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
            vec![
                (astro_token_instance.clone(), amount1),
                (usdc_token_instance.clone(), amount2),
            ],
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
            Uint128::from(854u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1170u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res[0].1, amount_exp);
    }
}

#[test]
fn consult_zero_price() {
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        Option::from(owner.clone()),
        Some(vec![
            Coin {
                denom: "cny".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ]),
    );
    let user = Addr::unchecked("user0000");

    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut router, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = vec![
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];

    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        vec![
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

    provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
        ],
    )
    .unwrap();

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
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res[0].1, amount_out);
    }

    let res: StdResult<Uint128> = router.wrap().query_wasm_smart(
        &oracle_instance,
        &Consult {
            token: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Default::default(),
        },
    );
    assert_eq!(
        res.unwrap_err().to_string(),
        "Generic error: Querier contract error: Generic error: Invalid Token"
    );

    // Consult zero price

    let asset_infos = vec![
        AssetInfo::NativeToken {
            denom: "cny".to_string(),
        },
        AssetInfo::NativeToken {
            denom: "uluna".to_string(),
        },
    ];

    let pair_info = create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100u8),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
        ],
    );

    provide_liquidity(
        &mut router,
        owner.clone(),
        user.clone(),
        &pair_info,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100u8),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
        ],
    )
    .unwrap();

    let oracle_instance = router
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &InstantiateMsg {
                factory_contract: factory_instance.to_string(),
                asset_infos: asset_infos.clone(),
            },
            &[],
            String::from("ORACLE 2"),
            None,
        )
        .unwrap();

    let res: Vec<(AssetInfo, Uint128)> = router
        .wrap()
        .query_wasm_smart(
            &oracle_instance,
            &Consult {
                token: asset_infos[1].clone(),
                amount: Uint128::from(1u8),
            },
        )
        .unwrap();
    // Price is too small thus we get zero
    assert_eq!(res[0].1.u128(), 0u128);
}

#[ignore]
#[test]
fn consult_multiple_assets() {
    let mut router = mock_app(None, None);
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

    let usdt_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdt token".to_string(),
        "USDT".to_string(),
    );

    let asset_infos = vec![
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: usdt_token_instance.clone(),
        },
    ];
    create_pair_stable(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        vec![
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(500_000_000000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(400_000_000000u128),
            },
            Asset {
                info: asset_infos[2].clone(),
                amount: Uint128::from(300_000_000000u128),
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
        vec![
            (
                usdc_token_instance.clone(),
                Uint128::from(500_000_000000u128),
            ),
            (
                astro_token_instance.clone(),
                Uint128::from(400_000_000000u128),
            ),
            (
                usdt_token_instance.clone(),
                Uint128::from(300_000_000000u128),
            ),
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
    assert_eq!(e.root_cause().to_string(), "Period not elapsed");
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
    for (amount1, amount2, amount3) in [
        (
            Uint128::from(500_000_000000u128),
            Uint128::from(400_000_000000u128),
            Uint128::from(300_000_000000u128),
        ),
        (
            Uint128::from(500_000_000000u128),
            Uint128::from(400_000_000000u128),
            Uint128::from(300_000_000000u128),
        ),
    ] {
        change_provide_liquidity(
            &mut router,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            vec![
                (usdc_token_instance.clone(), amount1),
                (astro_token_instance.clone(), amount2),
                (usdt_token_instance.clone(), amount3),
            ],
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
    for (addr, amount, amounts_exp) in [
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: astro_token_instance.clone(),
                    },
                    Uint128::from(997u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: usdt_token_instance.clone(),
                    },
                    Uint128::from(994u128),
                ),
            ],
        ),
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: usdc_token_instance.clone(),
                    },
                    Uint128::from(1002u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: usdt_token_instance.clone(),
                    },
                    Uint128::from(996u128),
                ),
            ],
        ),
        (
            usdt_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: usdc_token_instance.clone(),
                    },
                    Uint128::from(1005u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: astro_token_instance.clone(),
                    },
                    Uint128::from(1003u128),
                ),
            ],
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amounts_exp);
    }

    // Change pair liquidity
    for (amount1, amount2, amount3) in [
        (
            Uint128::from(100_000_000000u128),
            Uint128::from(95_000_000000u128),
            Uint128::from(100_000_000000u128),
        ),
        (
            Uint128::from(100_000_000000u128),
            Uint128::from(95_000_000000u128),
            Uint128::from(100_000_000000u128),
        ),
        (
            Uint128::from(100_000_000000u128),
            Uint128::from(95_000_000000u128),
            Uint128::from(100_000_000000u128),
        ),
    ] {
        change_provide_liquidity(
            &mut router,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            vec![
                (usdc_token_instance.clone(), amount1),
                (astro_token_instance.clone(), amount2),
                (usdt_token_instance.clone(), amount3),
            ],
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
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: astro_token_instance.clone(),
                    },
                    Uint128::from(998u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: usdt_token_instance.clone(),
                    },
                    Uint128::from(995u128),
                ),
            ],
        ),
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: usdc_token_instance.clone(),
                    },
                    Uint128::from(1001u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: usdt_token_instance.clone(),
                    },
                    Uint128::from(997u128),
                ),
            ],
        ),
        (
            usdt_token_instance.clone(),
            Uint128::from(1000u128),
            vec![
                (
                    AssetInfo::Token {
                        contract_addr: usdc_token_instance.clone(),
                    },
                    Uint128::from(1004u128),
                ),
                (
                    AssetInfo::Token {
                        contract_addr: astro_token_instance.clone(),
                    },
                    Uint128::from(1002u128),
                ),
            ],
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Vec<(AssetInfo, Uint128)> = router
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_exp);
    }
}
