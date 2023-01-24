use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType};
use astroport::pair::{
    ConfigResponse, Cw20HookMsg, InstantiateMsg as PairInstantiateMsg, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport::staking::{
    ConfigResponse as StakingConfigResponse, InstantiateMsg as StakingInstantiateMsg,
    QueryMsg as StakingQueryMsg,
};

use astroport::pair_bonded::{ExecuteMsg, QueryMsg};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_pair_astro_xastro::state::Params;
use cosmwasm_std::{to_binary, Addr, Coin, Uint128};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};

struct AstroportContracts {
    pair_instance: Addr,
    astro_instance: Addr,
    xastro_instance: Addr,
}

fn mock_app(owner: Addr, coins: Vec<Coin>) -> App {
    App::new(|router, _, storage| router.bank.init_balance(storage, &owner, coins).unwrap())
}

fn store_pair_code(app: &mut App) -> u64 {
    let pair_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_pair_astro_xastro::execute,
        astroport_pair_astro_xastro::instantiate,
        astroport_pair_astro_xastro::query,
    ));

    app.store_code(pair_contract)
}

fn store_staking_code(app: &mut App) -> u64 {
    let staking_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    );

    app.store_code(staking_contract)
}

fn store_astro_code(app: &mut App) -> u64 {
    let astro_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_contract)
}

fn store_xastro_code(app: &mut App) -> u64 {
    let xastro_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_xastro_token::contract::execute,
        astroport_xastro_token::contract::instantiate,
        astroport_xastro_token::contract::query,
    ));

    app.store_code(xastro_contract)
}

fn store_factory_code(app: &mut App) -> u64 {
    let factory_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_factory::contract::execute,
        astroport_factory::contract::instantiate,
        astroport_factory::contract::query,
    ));

    app.store_code(factory_contract)
}

fn instantiate_factory_contract(app: &mut App, owner: Addr, pair_code_id: u64) -> Addr {
    let code = store_factory_code(app);

    let msg = FactoryInstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            maker_fee_bps: 0,
            total_fee_bps: 0,
            pair_type: PairType::Custom("bonded".to_string()),
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id: 0,
        fee_address: None,
        generator_address: None,
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
    };

    app.instantiate_contract(
        code,
        owner,
        &msg,
        &[],
        String::from("Astroport Factory"),
        None,
    )
    .unwrap()
}

fn instantiate_token(app: &mut App, owner: Addr) -> Addr {
    let token_code_id = store_astro_code(app);

    let msg = TokenInstantiateMsg {
        name: "Astroport Token".to_string(),
        symbol: "ASTRO".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        owner.clone(),
        &msg,
        &[],
        String::from("Astroport Token"),
        None,
    )
    .unwrap()
}

fn instantiate_staking(app: &mut App, owner: Addr, token_instance: &Addr) -> (Addr, Addr) {
    let xastro_code_id = store_xastro_code(app);
    let staking_code_id = store_staking_code(app);

    let msg = StakingInstantiateMsg {
        owner: owner.to_string(),
        token_code_id: xastro_code_id,
        deposit_token_addr: token_instance.to_string(),
        marketing: None,
    };

    let staking_instance = app
        .instantiate_contract(
            staking_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("Astroport Staking"),
            None,
        )
        .unwrap();

    let resp: StakingConfigResponse = app
        .wrap()
        .query_wasm_smart(&staking_instance, &StakingQueryMsg::Config {})
        .unwrap();

    (staking_instance, resp.share_token_addr)
}

fn instantiate_astroport(mut router: &mut App, owner: &Addr) -> AstroportContracts {
    let pair_code_id = store_pair_code(&mut router);

    let factory_instance = instantiate_factory_contract(router, owner.clone(), pair_code_id);
    let token_instance = instantiate_token(router, owner.clone());

    let (staking_instance, xastro_instance) =
        instantiate_staking(router, owner.clone(), &token_instance);

    let msg = PairInstantiateMsg {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: xastro_instance.clone(),
            },
        ],
        token_code_id: 123,
        factory_addr: factory_instance.to_string(),
        init_params: Some(
            to_binary(&Params {
                astro_addr: token_instance.clone(),
                xastro_addr: xastro_instance.clone(),
                staking_addr: staking_instance.clone(),
            })
            .unwrap(),
        ),
    };

    let pair_instance = router
        .instantiate_contract(
            pair_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO-xASTRO pair"),
            None,
        )
        .unwrap();

    AstroportContracts {
        pair_instance,
        astro_instance: token_instance,
        xastro_instance,
    }
}

fn mint_tokens(router: &mut App, owner: Addr, token_addr: Addr, amount: Uint128, to: Addr) {
    router
        .execute_contract(
            owner,
            token_addr,
            &Cw20ExecuteMsg::Mint {
                recipient: to.to_string(),
                amount,
            },
            &[],
        )
        .unwrap();
}

fn assert_user_balance(router: &mut App, token: &Addr, user: &Addr, expected_balance: u64) {
    let balance: cw20::BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            token,
            &Cw20QueryMsg::Balance {
                address: user.to_string(),
            },
        )
        .unwrap();
    assert_eq!(balance.balance, Uint128::from(expected_balance));
}

#[test]
fn test_pair_instantiation() {
    let owner = Addr::unchecked("owner");

    let mut router = mock_app(owner.clone(), vec![]);

    let pair_code_id = store_pair_code(&mut router);

    let factory_instance = instantiate_factory_contract(&mut router, owner.clone(), pair_code_id);
    let token_instance = instantiate_token(&mut router, owner.clone());

    let (staking_instance, xastro_instance) =
        instantiate_staking(&mut router, owner.clone(), &token_instance);

    let msg = PairInstantiateMsg {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: xastro_instance.clone(),
            },
        ],
        token_code_id: 123,
        factory_addr: factory_instance.to_string(),
        init_params: None,
    };

    let err = router
        .instantiate_contract(
            pair_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO-xASTRO pair"),
            None,
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "You need to provide init params".to_string()
    );

    let msg = PairInstantiateMsg {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: xastro_instance.clone(),
            },
        ],
        token_code_id: 123,
        factory_addr: factory_instance.to_string(),
        init_params: Some(
            to_binary(&Params {
                astro_addr: token_instance.clone(),
                xastro_addr: xastro_instance.clone(),
                staking_addr: staking_instance.clone(),
            })
            .unwrap(),
        ),
    };

    let pair_instance = router
        .instantiate_contract(
            pair_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO-xASTRO pair"),
            None,
        )
        .unwrap();

    assert_eq!(factory_instance.to_string(), "contract0");
    assert_eq!(token_instance.to_string(), "contract1");
    assert_eq!(staking_instance.to_string(), "contract2");
    assert_eq!(xastro_instance.to_string(), "contract3");
    assert_eq!(pair_instance.to_string(), "contract4");
}

#[test]
fn test_pair_swap() {
    let owner = Addr::unchecked("owner");

    let user1 = Addr::unchecked("user1");
    let user2 = Addr::unchecked("user2");

    let mut router = mock_app(owner.clone(), vec![]);

    let contracts = instantiate_astroport(&mut router, &owner);

    // Mint ASTRO
    mint_tokens(
        &mut router,
        owner.clone(),
        contracts.astro_instance.clone(),
        Uint128::from(10_000u64),
        user1.clone(),
    );
    mint_tokens(
        &mut router,
        owner.clone(),
        contracts.astro_instance.clone(),
        Uint128::from(30_000u64),
        user2.clone(),
    );

    // Test simulate and reverse simulate with empty staking (ASTRO->xASTRO)
    let res: SimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::Simulation {
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.astro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        SimulationResponse {
            return_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );
    let res: ReverseSimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::ReverseSimulation {
                ask_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.xastro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        ReverseSimulationResponse {
            offer_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );

    // Test Swap operation ASTRO->xASTRO
    router
        .execute_contract(
            user1.clone(),
            contracts.astro_instance.clone(),
            &Cw20ExecuteMsg::Send {
                contract: contracts.pair_instance.clone().to_string(),
                amount: Uint128::from(10_000u64),
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();
    assert_user_balance(&mut router, &contracts.xastro_instance, &user1, 10_000u64);

    router
        .execute_contract(
            user2.clone(),
            contracts.astro_instance.clone(),
            &Cw20ExecuteMsg::Send {
                contract: contracts.pair_instance.clone().to_string(),
                amount: Uint128::from(30_000u64),
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();
    assert_user_balance(&mut router, &contracts.xastro_instance, &user2, 30_000u64);

    // Test simulate and reverse simulate (ASTRO->xASTRO)
    let res: SimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::Simulation {
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.astro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        SimulationResponse {
            return_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );
    let res: ReverseSimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::ReverseSimulation {
                ask_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.xastro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        ReverseSimulationResponse {
            offer_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );

    // Test simulate and reverse simulate (xASTRO->ASTRO)
    let res: SimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::Simulation {
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.xastro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        SimulationResponse {
            return_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );
    let res: ReverseSimulationResponse = router
        .wrap()
        .query_wasm_smart(
            &contracts.pair_instance,
            &QueryMsg::ReverseSimulation {
                ask_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.astro_instance.clone(),
                    },
                    amount: Uint128::from(10_000u64),
                },
            },
        )
        .unwrap();
    assert_eq!(
        res,
        ReverseSimulationResponse {
            offer_amount: Uint128::from(10000u64),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero()
        }
    );

    // Test Swap operation ASTRO->xASTRO
    router
        .execute_contract(
            user1.clone(),
            contracts.xastro_instance.clone(),
            &Cw20ExecuteMsg::Send {
                contract: contracts.pair_instance.clone().to_string(),
                amount: Uint128::from(10_000u64),
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();
    assert_user_balance(&mut router, &contracts.astro_instance, &user1, 10_000u64);

    router
        .execute_contract(
            user2.clone(),
            contracts.xastro_instance.clone(),
            &Cw20ExecuteMsg::Send {
                contract: contracts.pair_instance.clone().to_string(),
                amount: Uint128::from(30_000u64),
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap();
    assert_user_balance(&mut router, &contracts.astro_instance, &user2, 30_000u64);
}

#[test]
fn test_unsupported_methods() {
    let owner = Addr::unchecked("owner");

    let mut router = mock_app(owner.clone(), vec![]);

    let contracts = instantiate_astroport(&mut router, &owner);

    // Test provide liquidity
    let err = router
        .execute_contract(
            owner.clone(),
            contracts.pair_instance.clone(),
            &ExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: contracts.astro_instance.clone(),
                        },
                        amount: Uint128::from(100u64),
                    },
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: contracts.xastro_instance.clone(),
                        },
                        amount: Uint128::from(100u64),
                    },
                ],
                slippage_tolerance: None,
                auto_stake: None,
                receiver: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is not supported for this pool."
    );

    // Test update config
    let err = router
        .execute_contract(
            owner.clone(),
            contracts.pair_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                params: Default::default(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is not supported for this pool."
    );

    // Test update config
    let err = router
        .execute_contract(
            owner.clone(),
            contracts.pair_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                params: Default::default(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is not supported for this pool."
    );

    // Test native-swap
    let err = router
        .execute_contract(
            owner.clone(),
            contracts.pair_instance.clone(),
            &ExecuteMsg::Swap {
                offer_asset: Asset {
                    info: AssetInfo::Token {
                        contract_addr: contracts.astro_instance.clone(),
                    },
                    amount: Uint128::from(10u8),
                },
                belief_price: None,
                max_spread: None,
                to: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Operation is not supported for this pool."
    );
}

#[test]
fn test_queries() {
    let owner = Addr::unchecked("owner");

    let mut router = mock_app(owner.clone(), vec![]);

    let contracts = instantiate_astroport(&mut router, &owner);

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&contracts.pair_instance, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        res,
        ConfigResponse {
            block_time_last: 0u64,
            params: None,
        }
    );

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(&contracts.pair_instance, &QueryMsg::Pair {})
        .unwrap();
    assert_eq!(
        res,
        PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: contracts.astro_instance.clone()
                },
                AssetInfo::Token {
                    contract_addr: contracts.xastro_instance.clone()
                }
            ],
            contract_addr: contracts.pair_instance.clone(),
            liquidity_token: Addr::unchecked(""),
            pair_type: PairType::Custom("Bonded".to_string())
        }
    );
}
