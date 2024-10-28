#![cfg(not(tarpaulin_include))]

use astroport::asset::{native_asset_info, Asset, AssetInfo, PairInfo, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::factory::{
    ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType,
    QueryMsg as FactoryQueryMsg, TrackerConfig,
};
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
    TWAP_PRECISION,
};
use astroport::pair_xyk_sale_tax::{
    MigrateMsg, SaleTaxConfigUpdates, SaleTaxInitParams, TaxConfigUnchecked, TaxConfigsUnchecked,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport::tokenfactory_tracker::{
    ConfigResponse as TrackerConfigResponse, QueryMsg as TrackerQueryMsg,
};

use astroport::common::LP_SUBDENOM;
use astroport_pair_xyk_sale_tax::contract::CONTRACT_NAME;
use astroport_pair_xyk_sale_tax::error::ContractError;
use astroport_test::cw_multi_test::{AppBuilder, ContractWrapper, Executor, TOKEN_FACTORY_MODULE};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};
use cosmwasm_std::{
    attr, coin, to_json_binary, Addr, Coin, Decimal, DepsMut, Empty, Env, MessageInfo, Response,
    StdError, StdResult, Uint128,
};
use cw20::{Cw20Coin, Cw20ExecuteMsg, MinterResponse};
use test_case::test_case;

const OWNER: &str = "owner";

fn mock_app(owner: Addr, coins: Vec<Coin>) -> TestApp {
    AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|router, _, storage| router.bank.init_balance(storage, &owner, coins).unwrap())
}

fn store_token_code(app: &mut TestApp) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn store_standard_xyk_pair_code(app: &mut TestApp, version: &str) -> u64 {
    let code_id = match version {
        "1.3.1" => {
            let code = Box::new(
                ContractWrapper::new_with_empty(
                    astroport_pair_1_3_3::contract::execute,
                    astroport_pair_1_3_3::contract::instantiate,
                    astroport_pair_1_3_3::contract::query,
                )
                .with_migrate_empty(astroport_pair_1_3_3::contract::migrate)
                .with_reply_empty(astroport_pair_1_3_3::contract::reply),
            );
            app.store_code(code)
        }
        "1.5.0" => {
            let code = Box::new(
                ContractWrapper::new_with_empty(
                    astroport_pair::contract::execute,
                    astroport_pair::contract::instantiate,
                    astroport_pair::contract::query,
                )
                .with_reply_empty(astroport_pair::contract::reply),
            );
            app.store_code(code)
        }
        _ => panic!("Unsupported version"),
    };

    code_id
}

fn store_pair_code(app: &mut TestApp) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_xyk_sale_tax::contract::execute,
            astroport_pair_xyk_sale_tax::contract::instantiate,
            astroport_pair_xyk_sale_tax::contract::query,
        )
        .with_migrate_empty(astroport_pair_xyk_sale_tax::contract::migrate)
        .with_reply_empty(astroport_pair_xyk_sale_tax::contract::reply),
    );

    app.store_code(pair_contract)
}

fn store_factory_code(app: &mut TestApp) -> u64 {
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

fn store_generator_code(app: &mut TestApp) -> u64 {
    let generator_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_incentives::execute::execute,
        astroport_incentives::instantiate::instantiate,
        astroport_incentives::query::query,
    ));

    app.store_code(generator_contract)
}

fn store_tracker_contract(app: &mut TestApp) -> u64 {
    let tracker_contract = Box::new(
        ContractWrapper::new_with_empty(
            |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
                unimplemented!()
            },
            astroport_tokenfactory_tracker::contract::instantiate,
            astroport_tokenfactory_tracker::query::query,
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    );
    app.store_code(tracker_contract)
}

fn instantiate_pair(mut router: &mut TestApp, owner: &Addr) -> Addr {
    let token_contract_code_id = store_token_code(&mut router);

    let pair_contract_code_id = store_pair_code(&mut router);
    let factory_code_id = store_factory_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_contract_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id: token_contract_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: None,
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory_instance.to_string(),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
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

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Pair {})
        .unwrap();
    assert_eq!("contract1", res.contract_addr);
    assert_eq!(
        format!("factory/contract1/{LP_SUBDENOM}"),
        res.liquidity_token
    );

    pair
}

fn instantiate_standard_xyk_pair(mut router: &mut TestApp, owner: &Addr, version: &str) -> Addr {
    let token_contract_code_id = store_token_code(&mut router);

    let pair_contract_code_id = store_standard_xyk_pair_code(&mut router, version);
    let factory_code_id = store_factory_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_contract_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Xyk {},
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id: token_contract_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: None,
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory_instance.to_string(),
        init_params: None,
    };

    let pair = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
            Some(owner.to_string()),
        )
        .unwrap();

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Pair {})
        .unwrap();
    assert_eq!("contract1", res.contract_addr);
    assert_eq!(
        format!("factory/contract1/{}", LP_SUBDENOM),
        res.liquidity_token
    );

    pair
}

#[test]
fn test_provide_and_withdraw_liquidity() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "cny".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Set Alice's balances
    router
        .send_tokens(
            owner.clone(),
            alice_address.clone(),
            &[
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(233_000_000u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(2_00_000_000u128),
                },
                Coin {
                    denom: "cny".to_string(),
                    amount: Uint128::from(100_000_000u128),
                },
            ],
        )
        .unwrap();

    // Init pair
    let pair_instance = instantiate_pair(&mut router, &owner);

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair_instance.to_string(), &QueryMsg::Pair {})
        .unwrap();
    let lp_token = res.liquidity_token;

    assert_eq!(
        res.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
    );

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100_000_000),
        Uint128::new(100_000_000),
        None,
        None,
    );
    let res = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "provide_liquidity")
    );
    assert_eq!(res.events[1].attributes[3], attr("receiver", "alice"),);
    assert_eq!(
        res.events[1].attributes[4],
        attr("assets", "100000000uusd, 100000000uluna")
    );
    assert_eq!(
        res.events[1].attributes[5],
        attr("share", 99999000u128.to_string())
    );

    // Provide liquidity for receiver
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100),
        Uint128::new(100),
        Some("bob".to_string()),
        None,
    );
    let res = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "provide_liquidity")
    );
    assert_eq!(res.events[1].attributes[3], attr("receiver", "bob"),);
    assert_eq!(
        res.events[1].attributes[4],
        attr("assets", "100uusd, 100uluna")
    );
    assert_eq!(
        res.events[1].attributes[5],
        attr("share", 100u128.to_string())
    );

    let msg = ExecuteMsg::WithdrawLiquidity {
        assets: vec![],
        min_assets_to_receive: None,
    };

    // Try to send withdraw liquidity with uluna token
    let err = router
        .execute_contract(
            alice_address.clone(),
            pair_instance.clone(),
            &msg,
            &[coin(50u128, "uluna")],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Must send reserve token 'factory/contract1/{}'",
            LP_SUBDENOM
        )
    );

    // Withdraw with LP token is successful
    router
        .execute_contract(
            alice_address.clone(),
            pair_instance.clone(),
            &msg,
            &[coin(50u128, lp_token.clone())],
        )
        .unwrap();

    let err = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "No funds sent");

    // Check pair config
    let config: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair_instance.to_string(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        config.clone(),
        ConfigResponse {
            block_time_last: router.block_info().time.seconds(),
            params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
            owner,
            factory_addr: config.factory_addr,
            tracker_addr: config.tracker_addr
        }
    )
}

fn provide_liquidity_msg(
    uusd_amount: Uint128,
    uluna_amount: Uint128,
    receiver: Option<String>,
    slippage_tolerance: Option<Decimal>,
) -> (ExecuteMsg, [Coin; 2]) {
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: uusd_amount.clone(),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                amount: uluna_amount.clone(),
            },
        ],
        slippage_tolerance: Option::from(slippage_tolerance),
        auto_stake: None,
        receiver,
        min_lp_to_receive: None,
    };

    let coins = [
        Coin {
            denom: "uluna".to_string(),
            amount: uluna_amount.clone(),
        },
        Coin {
            denom: "uusd".to_string(),
            amount: uusd_amount.clone(),
        },
    ];

    (msg, coins)
}

#[test]
fn simulate_provide() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");

    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Set Alice's balances
    router
        .send_tokens(
            owner.clone(),
            alice_address.clone(),
            &[
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(233_000_000u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(2_00_000_000u128),
                },
            ],
        )
        .unwrap();

    let pair_instance = instantiate_pair(&mut router, &owner);

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair_instance.to_string(), &QueryMsg::Pair {})
        .unwrap();

    let lp_token = res.liquidity_token;

    assert_eq!(
        res.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
    );

    let assets = vec![
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::new(100_000_000),
        },
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Uint128::new(100_000_000),
        },
    ];

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100_000_000),
        Uint128::new(100_000_000),
        None,
        None,
    );

    let shares: Uint128 = router
        .wrap()
        .query_wasm_smart(
            pair_instance.to_string(),
            &QueryMsg::SimulateProvide {
                assets: assets.clone(),
                slippage_tolerance: None,
            },
        )
        .unwrap();

    router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    let user_balance = router
        .wrap()
        .query_balance(alice_address, lp_token)
        .unwrap();
    assert_eq!(shares, user_balance.amount);

    let assets = vec![
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::new(100_000_0000u128),
        },
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Uint128::new(50_000_000000u128),
        },
    ];

    let err = router
        .wrap()
        .query_wasm_smart::<Uint128>(
            pair_instance.to_string(),
            &QueryMsg::SimulateProvide {
                assets: assets.clone(),
                slippage_tolerance: Option::from(Decimal::percent(1)),
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        StdError::generic_err(
            "Querier contract error: Generic error: Operation exceeds max splippage tolerance"
        )
    );
}

#[test]
fn test_compatibility_of_tokens_with_different_precision() {
    let owner = Addr::unchecked(OWNER);

    let mut app = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000000u128),
            },
        ],
    );

    let token_code_id = store_token_code(&mut app);

    let x_amount = Uint128::new(1000000_00000);
    let y_amount = Uint128::new(1000000_0000000);
    let x_offer = Uint128::new(1_00000);
    let y_expected_return = Uint128::new(1_0000000);

    let token_name = "Xtoken";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 5,
        initial_balances: vec![Cw20Coin {
            address: OWNER.to_string(),
            amount: x_amount + x_offer,
        }],
        mint: Some(MinterResponse {
            minter: String::from(OWNER),
            cap: None,
        }),
        marketing: None,
    };

    let token_x_instance = app
        .instantiate_contract(
            token_code_id,
            owner.clone(),
            &init_msg,
            &[],
            token_name,
            None,
        )
        .unwrap();

    let pair_code_id = store_pair_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: None,
    };

    let factory_instance = app
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        asset_infos: vec![
            AssetInfo::Token {
                contract_addr: token_x_instance.clone(),
            },
            AssetInfo::native("uusd"),
        ],
        pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    let msg = FactoryQueryMsg::Pair {
        asset_infos: vec![
            AssetInfo::Token {
                contract_addr: token_x_instance.clone(),
            },
            AssetInfo::native("uusd"),
        ],
    };

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(&factory_instance, &msg)
        .unwrap();

    let pair_instance = res.contract_addr;

    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: pair_instance.to_string(),
        expires: None,
        amount: x_amount + x_offer,
    };

    app.execute_contract(owner.clone(), token_x_instance.clone(), &msg, &[])
        .unwrap();

    let user = Addr::unchecked("user");

    let swap_msg = Cw20ExecuteMsg::Send {
        contract: pair_instance.to_string(),
        msg: to_json_binary(&Cw20HookMsg::Swap {
            ask_asset_info: None,
            belief_price: None,
            max_spread: None,
            to: Some(user.to_string()),
        })
        .unwrap(),
        amount: x_offer,
    };

    let err = app
        .execute_contract(owner.clone(), token_x_instance.clone(), &swap_msg, &[])
        .unwrap_err();
    assert_eq!(
        "Generic error: One of the pools is empty",
        err.root_cause().to_string()
    );

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_x_instance.clone(),
                },
                amount: x_amount,
            },
            Asset {
                info: AssetInfo::native("uusd"),
                amount: y_amount,
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
        min_lp_to_receive: None,
    };

    app.execute_contract(
        owner.clone(),
        pair_instance.clone(),
        &msg,
        &[coin(y_amount.u128(), "uusd")],
    )
    .unwrap();

    let user = Addr::unchecked("user");

    let swap_msg = Cw20ExecuteMsg::Send {
        contract: pair_instance.to_string(),
        msg: to_json_binary(&Cw20HookMsg::Swap {
            ask_asset_info: None,
            belief_price: None,
            max_spread: None,
            to: Some(user.to_string()),
        })
        .unwrap(),
        amount: x_offer,
    };

    // try to swap after provide liquidity
    app.execute_contract(owner.clone(), token_x_instance.clone(), &swap_msg, &[])
        .unwrap();

    let res = app.wrap().query_balance(&user, "uusd").unwrap();

    let acceptable_spread_amount = Uint128::new(10);

    assert_eq!(res.amount, y_expected_return - acceptable_spread_amount);
}

#[test]
fn test_if_twap_is_calculated_correctly_when_pool_idles() {
    let owner = Addr::unchecked("owner");
    let user1 = Addr::unchecked("user1");

    let mut app = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000000u128),
            },
        ],
    );

    // Set Alice's balances
    app.send_tokens(
        owner.clone(),
        user1.clone(),
        &[
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(4000000_000000),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(2000000_000000),
            },
        ],
    )
    .unwrap();

    // Instantiate pair
    let pair_instance = instantiate_pair(&mut app, &user1);

    // Provide liquidity, accumulators are empty
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(1000000_000000),
        Uint128::new(1000000_000000),
        None,
        Option::from(Decimal::one()),
    );
    app.execute_contract(user1.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    const BLOCKS_PER_DAY: u64 = 17280;
    const ELAPSED_SECONDS: u64 = BLOCKS_PER_DAY * 5;

    // A day later
    app.update_block(|b| {
        b.height += BLOCKS_PER_DAY;
        b.time = b.time.plus_seconds(ELAPSED_SECONDS);
    });

    // Provide liquidity, accumulators firstly filled with the same prices
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(2000000_000000),
        Uint128::new(1000000_000000),
        None,
        Some(Decimal::percent(50)),
    );
    app.execute_contract(user1.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    // Get current twap accumulator values
    let msg = QueryMsg::CumulativePrices {};
    let cpr_old: CumulativePricesResponse =
        app.wrap().query_wasm_smart(&pair_instance, &msg).unwrap();

    // A day later
    app.update_block(|b| {
        b.height += BLOCKS_PER_DAY;
        b.time = b.time.plus_seconds(ELAPSED_SECONDS);
    });

    // Get current cumulative price values; they should have been updated by the query method with new 2/1 ratio
    let msg = QueryMsg::CumulativePrices {};
    let cpr_new: CumulativePricesResponse =
        app.wrap().query_wasm_smart(&pair_instance, &msg).unwrap();

    let twap0 = cpr_new.cumulative_prices[0].2 - cpr_old.cumulative_prices[0].2;
    let twap1 = cpr_new.cumulative_prices[1].2 - cpr_old.cumulative_prices[1].2;

    // Prices weren't changed for the last day, uusd amount in pool = 3000000_000000, uluna = 2000000_000000
    // In accumulators we don't have any precision so we rely on elapsed time so we don't need to consider it
    let price_precision = Uint128::from(10u128.pow(TWAP_PRECISION.into()));
    assert_eq!(twap0 / price_precision, Uint128::new(57600)); // 0.666666 * ELAPSED_SECONDS (86400)
    assert_eq!(twap1 / price_precision, Uint128::new(129600)); //   1.5 * ELAPSED_SECONDS
}

#[test]
fn create_pair_with_same_assets() {
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    let token_contract_code_id = store_token_code(&mut router);
    let pair_contract_code_id = store_pair_code(&mut router);

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: String::from("factory"),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
    };

    let resp = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
            None,
        )
        .unwrap_err();

    assert_eq!(
        resp.root_cause().to_string(),
        "Doubling assets in asset infos"
    )
}

#[test]
fn wrong_number_of_assets() {
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(owner.clone(), vec![]);

    let pair_contract_code_id = store_pair_code(&mut router);

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        }],
        token_code_id: 123,
        factory_addr: String::from("factory"),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
    };

    let err = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
            None,
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: asset_infos must contain exactly two elements"
    );

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            native_asset_info("uusd".to_string()),
            native_asset_info("dust".to_string()),
            native_asset_info("stone".to_string()),
        ],
        token_code_id: 123,
        factory_addr: String::from("factory"),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
    };

    let err = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
            None,
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: asset_infos must contain exactly two elements"
    );
}

#[test]
fn asset_balances_tracking_works_correctly() {
    let owner = Addr::unchecked("owner");
    let mut app = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "test1".to_owned(),
                amount: Uint128::new(5_000000),
            },
            Coin {
                denom: "test2".to_owned(),
                amount: Uint128::new(5_000000),
            },
            Coin {
                denom: "uluna".to_owned(),
                amount: Uint128::new(1000_000000),
            },
            Coin {
                denom: "uusd".to_owned(),
                amount: Uint128::new(1000_000000),
            },
        ],
    );
    let token_code_id = store_token_code(&mut app);
    let pair_code_id = store_pair_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: Some(TrackerConfig {
            code_id: store_tracker_contract(&mut app),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
        }),
    };

    let factory_instance = app
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    // Instantiate new pair with asset balances tracking starting from instantiation
    let msg = FactoryExecuteMsg::CreatePair {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
        pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
        init_params: Some(
            to_json_binary(&SaleTaxInitParams {
                track_asset_balances: true,
                tax_configs: TaxConfigsUnchecked::new(),
                tax_config_admin: "tax_config_admin".to_string(),
            })
            .unwrap(),
        ),
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    let msg = FactoryQueryMsg::Pair {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    };

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(&factory_instance, &msg)
        .unwrap();

    let pair_instance = res.contract_addr;
    let lp_token_address = res.liquidity_token;

    // Check that asset balances were not tracked before instantiation
    // The query AssetBalanceAt returns None for this case
    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.is_none());

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.is_none());

    // Check that asset balances were not tracked before instantiation
    // The query AssetBalanceAt returns None for this case
    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.is_none());

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.is_none());

    // Check that asset balances had zero balances before next block upon instantiation
    app.update_block(|b| b.height += 1);

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.unwrap().is_zero());

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert!(res.unwrap().is_zero());

    // Provide liquidity
    let (msg, send_funds) = provide_liquidity_msg(
        Uint128::new(999_000000),
        Uint128::new(1000_000000),
        None,
        None,
    );
    app.execute_contract(owner.clone(), pair_instance.clone(), &msg, &send_funds)
        .unwrap();

    let owner_lp_balance = app
        .wrap()
        .query_balance(owner.to_string(), &lp_token_address)
        .unwrap();
    assert_eq!(owner_lp_balance.amount, Uint128::new(999498874));

    // Check that asset balances changed after providing liqudity
    app.update_block(|b| b.height += 1);
    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(1000_000000));

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(999_000000));

    // Swap

    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_owned(),
            },
            amount: Uint128::new(1_000000),
        },
        ask_asset_info: None,
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let send_funds = vec![Coin {
        denom: "uusd".to_owned(),
        amount: Uint128::new(1_000000),
    }];
    app.execute_contract(owner.clone(), pair_instance.clone(), &msg, &send_funds)
        .unwrap();

    // Check that asset balances changed after swaping
    app.update_block(|b| b.height += 1);
    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(999_000000));

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(1000_000000));

    // Withdraw liqudity
    app.execute_contract(
        owner.clone(),
        pair_instance.clone(),
        &ExecuteMsg::WithdrawLiquidity {
            assets: vec![],
            min_assets_to_receive: None,
        },
        &[coin(500_000000u128, lp_token_address)],
    )
    .unwrap();

    // Check that asset balances changed after withdrawing
    app.update_block(|b| b.height += 1);
    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(499_250063));

    let res: Option<Uint128> = app
        .wrap()
        .query_wasm_smart(
            &pair_instance,
            &QueryMsg::AssetBalanceAt {
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_owned(),
                },
                block_height: app.block_info().height.into(),
            },
        )
        .unwrap();
    assert_eq!(res.unwrap(), Uint128::new(499_749812));
}

#[test]
fn update_pair_config() {
    let owner = Addr::unchecked(OWNER);
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    let token_contract_code_id = store_token_code(&mut router);
    let pair_contract_code_id = store_pair_code(&mut router);

    let factory_code_id = store_factory_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![],
        token_code_id: token_contract_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: Some(TrackerConfig {
            code_id: store_tracker_contract(&mut router),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
        }),
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory_instance.to_string(),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
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

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(
        res,
        ConfigResponse {
            block_time_last: 0,
            params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
            owner: Addr::unchecked("owner"),
            factory_addr: Addr::unchecked("contract0"),
            tracker_addr: None
        }
    );
}

#[test]
fn update_tax_configs() {
    let owner = Addr::unchecked(OWNER);
    let default_tax_config_admin = Addr::unchecked("addr0000");

    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    let token_contract_code_id = store_token_code(&mut router);
    let pair_contract_code_id = store_pair_code(&mut router);

    let factory_code_id = store_factory_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![],
        token_code_id: token_contract_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: None,
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = InstantiateMsg {
        pair_type: PairType::Custom(CONTRACT_NAME.to_string()),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory_instance.to_string(),
        init_params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
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

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(
        res,
        ConfigResponse {
            block_time_last: 0,
            params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
            owner: Addr::unchecked("owner"),
            factory_addr: Addr::unchecked("contract0"),
            tracker_addr: None
        }
    );

    // Update tax config admin
    let msg = ExecuteMsg::UpdateConfig {
        params: to_json_binary(&SaleTaxConfigUpdates {
            tax_config_admin: Some("new_admin".to_string()),
            ..Default::default()
        })
        .unwrap(),
    };
    assert_eq!(
        router
            .execute_contract(owner.clone(), pair.clone(), &msg, &[])
            .unwrap_err()
            .downcast_ref::<ContractError>()
            .unwrap(),
        &ContractError::Unauthorized {}
    );

    router
        .execute_contract(default_tax_config_admin, pair.clone(), &msg, &[])
        .unwrap();

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        res,
        ConfigResponse {
            block_time_last: 0,
            params: Some(
                to_json_binary(&SaleTaxInitParams {
                    tax_config_admin: "new_admin".to_string(),
                    ..Default::default()
                })
                .unwrap()
            ),
            owner: Addr::unchecked("owner"),
            factory_addr: Addr::unchecked("contract0"),
            tracker_addr: None
        }
    );

    // Update tax configs
    let new_tax_configs: TaxConfigsUnchecked = vec![(
        "uluna",
        TaxConfigUnchecked {
            tax_rate: Decimal::percent(42),
            tax_recipient: "new_recipient".to_string(),
        },
    )]
    .into();
    let msg = ExecuteMsg::UpdateConfig {
        params: to_json_binary(&SaleTaxConfigUpdates {
            tax_configs: Some(new_tax_configs.clone()),
            ..Default::default()
        })
        .unwrap(),
    };
    assert_eq!(
        router
            .execute_contract(owner, pair.clone(), &msg, &[])
            .unwrap_err()
            .downcast_ref::<ContractError>()
            .unwrap(),
        &ContractError::Unauthorized {}
    );

    router
        .execute_contract(Addr::unchecked("new_admin"), pair.clone(), &msg, &[])
        .unwrap();

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        res,
        ConfigResponse {
            block_time_last: 0,
            params: Some(
                to_json_binary(&SaleTaxInitParams {
                    tax_configs: new_tax_configs,
                    tax_config_admin: "new_admin".to_string(),
                    ..Default::default()
                })
                .unwrap()
            ),
            owner: Addr::unchecked("owner"),
            factory_addr: Addr::unchecked("contract0"),
            tracker_addr: None
        }
    );
}

#[test]
fn provide_liquidity_with_autostaking_to_generator() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "cny".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Set Alice's balances
    router
        .send_tokens(
            owner.clone(),
            alice_address.clone(),
            &[
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(233_000_000u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(2_00_000_000u128),
                },
                Coin {
                    denom: "cny".to_string(),
                    amount: Uint128::from(100_000_000u128),
                },
            ],
        )
        .unwrap();

    let token_contract_code_id = store_token_code(&mut router);

    let pair_contract_code_id = store_pair_code(&mut router);
    let factory_code_id = store_factory_code(&mut router);

    let generator_code_id = store_generator_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_contract_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id: token_contract_code_id,
        generator_address: None,
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: Some(TrackerConfig {
            code_id: store_tracker_contract(&mut router),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
        }),
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let generator_instance = router
        .instantiate_contract(
            generator_code_id,
            owner.clone(),
            &astroport::incentives::InstantiateMsg {
                astro_token: native_asset_info("astro".to_string()),
                factory: factory_instance.to_string(),
                owner: owner.to_string(),
                guardian: None,
                incentivization_fee_info: None,
                vesting_contract: "vesting".to_string(),
            },
            &[],
            "generator",
            None,
        )
        .unwrap();

    router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &astroport::factory::ExecuteMsg::UpdateConfig {
                token_code_id: None,
                fee_address: None,
                generator_address: Some(generator_instance.to_string()),
                whitelist_code_id: None,
                coin_registry_address: None,
            },
            &[],
        )
        .unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
        init_params: Some(
            to_json_binary(&SaleTaxInitParams {
                track_asset_balances: true,
                tax_configs: TaxConfigsUnchecked::new(),
                tax_config_admin: "tax_config_admin".to_string(),
            })
            .unwrap(),
        ),
    };

    router
        .execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    let uusd_amount = Uint128::new(100_000_000);
    let uluna_amount = Uint128::new(100_000_000);

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: uusd_amount.clone(),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                amount: uluna_amount.clone(),
            },
        ],
        slippage_tolerance: None,
        auto_stake: Some(true),
        receiver: None,
        min_lp_to_receive: None,
    };

    let coins = [
        Coin {
            denom: "uluna".to_string(),
            amount: uluna_amount.clone(),
        },
        Coin {
            denom: "uusd".to_string(),
            amount: uusd_amount.clone(),
        },
    ];

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(
            &factory_instance,
            &FactoryQueryMsg::Pair {
                asset_infos: vec![
                    AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
            },
        )
        .unwrap();

    let pair_instance = res.contract_addr;
    let lp_token_address = res.liquidity_token;

    router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    let amount: Uint128 = router
        .wrap()
        .query_wasm_smart(
            generator_instance.to_string(),
            &astroport::incentives::QueryMsg::Deposit {
                lp_token: lp_token_address.to_string(),
                user: alice_address.to_string(),
            },
        )
        .unwrap();

    assert_eq!(amount, Uint128::new(99999000));
}

#[test]
fn test_tracker_contract() {
    let owner = Addr::unchecked("owner");
    let alice = Addr::unchecked("alice");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    let pair_contract_code_id = store_pair_code(&mut router);
    let factory_code_id = store_factory_code(&mut router);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_contract_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
            total_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id: 0,
        generator_address: None,
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
        coin_registry_address: "coin_registry".to_string(),
        tracker_config: Some(TrackerConfig {
            code_id: store_tracker_contract(&mut router),
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
        }),
    };

    let factory_instance = router
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        pair_type: PairType::Custom(env!("CARGO_PKG_NAME").to_string()),
        init_params: Some(
            to_json_binary(&SaleTaxInitParams {
                track_asset_balances: true,
                tax_configs: TaxConfigsUnchecked::new(),
                tax_config_admin: "tax_config_admin".to_string(),
            })
            .unwrap(),
        ),
    };

    router
        .execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    let uusd_amount = Uint128::new(100_000_000);
    let uluna_amount = Uint128::new(100_000_000);

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: uusd_amount.clone(),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                amount: uluna_amount.clone(),
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
        min_lp_to_receive: None,
    };

    let coins = [
        Coin {
            denom: "uluna".to_string(),
            amount: uluna_amount.clone(),
        },
        Coin {
            denom: "uusd".to_string(),
            amount: uusd_amount.clone(),
        },
    ];

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(
            &factory_instance,
            &FactoryQueryMsg::Pair {
                asset_infos: vec![
                    AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
            },
        )
        .unwrap();

    let pair_instance = res.contract_addr;
    let lp_token = res.liquidity_token;

    router
        .execute_contract(owner.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    let owner_lp_funds = router
        .wrap()
        .query_balance(owner.clone(), lp_token.clone())
        .unwrap();

    let total_supply = owner_lp_funds.amount + MINIMUM_LIQUIDITY_AMOUNT;

    // Set Alice's balances
    router
        .send_tokens(
            owner.clone(),
            alice.clone(),
            &[Coin {
                denom: lp_token.to_string(),
                amount: Uint128::new(100),
            }],
        )
        .unwrap();

    let config: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair_instance.clone(), &QueryMsg::Config {})
        .unwrap();

    let tracker_addr = config.tracker_addr.unwrap();

    let tracker_config: TrackerConfigResponse = router
        .wrap()
        .query_wasm_smart(tracker_addr.clone(), &TrackerQueryMsg::Config {})
        .unwrap();
    assert_eq!(
        tracker_config.token_factory_module,
        TOKEN_FACTORY_MODULE.to_string()
    );
    assert_eq!(tracker_config.tracked_denom, lp_token.to_string());

    let tracker_total_supply: Uint128 = router
        .wrap()
        .query_wasm_smart(
            tracker_addr.clone(),
            &TrackerQueryMsg::TotalSupplyAt { unit: None },
        )
        .unwrap();

    assert_eq!(total_supply, tracker_total_supply);

    let alice_balance: Uint128 = router
        .wrap()
        .query_wasm_smart(
            tracker_addr,
            &TrackerQueryMsg::BalanceAt {
                address: alice.to_string(),
                unit: None,
            },
        )
        .unwrap();

    assert_eq!(alice_balance, Uint128::new(100));
}

#[test]
fn test_imbalanced_withdraw_is_disabled() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Set Alice's balances
    router
        .send_tokens(
            owner.clone(),
            alice_address.clone(),
            &[
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(233_000_000u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(2_00_000_000u128),
                },
            ],
        )
        .unwrap();

    // Init pair
    let pair_instance = instantiate_pair(&mut router, &owner);

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair_instance.to_string(), &QueryMsg::Pair {})
        .unwrap();
    let lp_token = res.liquidity_token;

    assert_eq!(
        res.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
    );

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100_000_000),
        Uint128::new(100_000_000),
        None,
        None,
    );
    router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    // Provide liquidity for receiver
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100),
        Uint128::new(100),
        Some("bob".to_string()),
        None,
    );
    router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    // Check that imbalanced withdraw is currently disabled
    let msg_imbalance = ExecuteMsg::WithdrawLiquidity {
        assets: vec![Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100u8),
        }],
        min_assets_to_receive: None,
    };

    let err = router
        .execute_contract(
            alice_address.clone(),
            pair_instance.clone(),
            &msg_imbalance,
            &[coin(100u128, lp_token)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Imbalanced withdraw is currently disabled"
    );
}

#[ignore]
#[test_case("1.3.1"; "v1.3.1")]
#[test_case("1.5.0"; "v1.5.0")]
fn test_migrate_from_standard_xyk(old_version: &str) {
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Init pair
    let pair_instance = instantiate_standard_xyk_pair(&mut router, &owner, old_version);

    // Store xyk_sale_tax wasm
    let xyk_sale_tax_code_id = store_pair_code(&mut router);

    // Migrate pair
    let msg = MigrateMsg {
        tax_config_admin: "addr0000".to_string(),
        tax_configs: TaxConfigsUnchecked::default(),
    };
    router
        .migrate_contract(
            owner.clone(),
            pair_instance.clone(),
            &msg,
            xyk_sale_tax_code_id,
        )
        .unwrap();

    // Query config
    let config: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair_instance.to_string(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        config.clone(),
        ConfigResponse {
            block_time_last: 0,
            params: Some(to_json_binary(&SaleTaxInitParams::default()).unwrap()),
            owner,
            factory_addr: config.factory_addr,
            tracker_addr: None
        }
    )
}
