use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{
    ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType,
    QueryMsg as FactoryQueryMsg,
};
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
    StablePoolConfig, StablePoolParams, StablePoolUpdateParams, TWAP_PRECISION,
};

use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_pair_stable::math::{MAX_AMP, MAX_AMP_CHANGE, MIN_AMP_CHANGING_TIME};
use cosmwasm_std::{
    attr, coin, from_binary, to_binary, Addr, Coin, Decimal, QueryRequest, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, BasicApp, ContractWrapper, Executor};

const OWNER: &str = "owner";

type TerraApp = App;
fn mock_app() -> TerraApp {
    BasicApp::default()
}

fn store_token_code(app: &mut TerraApp) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn store_pair_code(app: &mut TerraApp) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_stable::contract::execute,
            astroport_pair_stable::contract::instantiate,
            astroport_pair_stable::contract::query,
        )
        .with_reply_empty(astroport_pair_stable::contract::reply),
    );

    app.store_code(pair_contract)
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

fn instantiate_pair(mut router: &mut TerraApp, owner: &Addr) -> Addr {
    let token_contract_code_id = store_token_code(&mut router);

    let pair_contract_code_id = store_pair_code(&mut router);

    let msg = InstantiateMsg {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: String::from("factory"),
        init_params: None,
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
        "You need to provide init params",
        resp.root_cause().to_string()
    );

    let msg = InstantiateMsg {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: String::from("factory"),
        init_params: Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
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
    assert_eq!("contract0", res.contract_addr.as_str());
    assert_eq!("contract1", res.liquidity_token.as_str());

    pair
}

#[test]
fn test_provide_and_withdraw_liquidity() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = TerraApp::new(|router, _, storage| {
        // initialization moved to App construction
        router
            .bank
            .init_balance(
                storage,
                &alice_address,
                vec![
                    Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(233000u128),
                    },
                    Coin {
                        denom: "uluna".to_string(),
                        amount: Uint128::new(200000u128),
                    },
                ],
            )
            .unwrap();
        router
            .bank
            .init_balance(
                storage,
                &owner,
                vec![
                    Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000000u128),
                    },
                    Coin {
                        denom: "uluna".to_string(),
                        amount: Uint128::new(100_000000u128),
                    },
                ],
            )
            .unwrap()
    });

    // Init pair
    let pair_instance = instantiate_pair(&mut router, &owner);

    let res: Result<PairInfo, _> = router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_instance.to_string(),
        msg: to_binary(&QueryMsg::Pair {}).unwrap(),
    }));
    let res = res.unwrap();

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

    // When dealing with native tokens transfer should happen before contract call, which cw-multitest doesn't support
    router
        .send_tokens(
            owner.clone(),
            pair_instance.clone(),
            &[coin(100000, "uusd"), coin(100000, "uluna")],
        )
        .unwrap();

    // Try to provide liquidity less then MINIMUM_LIQUIDITY_AMOUNT
    let (msg, coins) = provide_liquidity_msg(Uint128::new(100), Uint128::new(100), None);
    let err = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap_err();
    assert_eq!(
        "Initial liquidity must be more than 1000",
        err.root_cause().to_string()
    );

    // Try to provide liquidity equal to MINIMUM_LIQUIDITY_AMOUNT
    let (msg, coins) = provide_liquidity_msg(Uint128::new(1000), Uint128::new(1000), None);
    let err = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap_err();
    assert_eq!(
        "Initial liquidity must be more than 1000",
        err.root_cause().to_string()
    );

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(Uint128::new(100000), Uint128::new(100000), None);
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
        attr("assets", "100000uusd, 100000uluna")
    );
    assert_eq!(
        res.events[1].attributes[5],
        attr("share", 99000u128.to_string())
    );
    assert_eq!(res.events[3].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[3].attributes[2], attr("to", "contract0"));
    assert_eq!(
        res.events[3].attributes[3],
        attr("amount", 1000.to_string())
    );

    assert_eq!(res.events[5].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[5].attributes[2], attr("to", "alice"));
    assert_eq!(
        res.events[5].attributes[3],
        attr("amount", 99000.to_string())
    );

    // Provide liquidity for receiver
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100000),
        Uint128::new(100000),
        Some("bob".to_string()),
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
        attr("assets", "100000uusd, 100000uluna")
    );
    assert_eq!(
        res.events[1].attributes[5],
        attr("share", 50000u128.to_string())
    );
    assert_eq!(res.events[3].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[3].attributes[2], attr("to", "bob"));
    assert_eq!(
        res.events[3].attributes[3],
        attr("amount", 50000.to_string())
    );
}

fn provide_liquidity_msg(
    uusd_amount: Uint128,
    uluna_amount: Uint128,
    receiver: Option<String>,
) -> (ExecuteMsg, [Coin; 2]) {
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: [
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
        receiver,
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
fn test_compatibility_of_tokens_with_different_precision() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);

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

    let token_name = "Ytoken";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 7,
        initial_balances: vec![Cw20Coin {
            address: OWNER.to_string(),
            amount: y_amount,
        }],
        mint: Some(MinterResponse {
            minter: String::from(OWNER),
            cap: None,
        }),
        marketing: None,
    };

    let token_y_instance = app
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
            total_fee_bps: 0,
            pair_type: PairType::Stable {},
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id,
        generator_address: Some(String::from("generator")),
        owner: String::from("owner0000"),
        whitelist_code_id: 234u64,
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
        pair_type: PairType::Stable {},
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_x_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: token_y_instance.clone(),
            },
        ],
        init_params: Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
    };

    app.execute_contract(owner.clone(), factory_instance.clone(), &msg, &[])
        .unwrap();

    let msg = FactoryQueryMsg::Pair {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: token_x_instance.clone(),
            },
            AssetInfo::Token {
                contract_addr: token_y_instance.clone(),
            },
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

    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: pair_instance.to_string(),
        expires: None,
        amount: y_amount,
    };

    app.execute_contract(owner.clone(), token_y_instance.clone(), &msg, &[])
        .unwrap();

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_x_instance.clone(),
                },
                amount: x_amount,
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_y_instance.clone(),
                },
                amount: y_amount,
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    app.execute_contract(owner.clone(), pair_instance.clone(), &msg, &[])
        .unwrap();

    let user = Addr::unchecked("user");

    let msg = Cw20ExecuteMsg::Send {
        contract: pair_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: Some(user.to_string()),
        })
        .unwrap(),
        amount: x_offer,
    };

    app.execute_contract(owner.clone(), token_x_instance.clone(), &msg, &[])
        .unwrap();

    let msg = Cw20QueryMsg::Balance {
        address: user.to_string(),
    };

    let res: BalanceResponse = app
        .wrap()
        .query_wasm_smart(&token_y_instance, &msg)
        .unwrap();

    assert_eq!(res.balance, y_expected_return);
}

#[test]
fn test_if_twap_is_calculated_correctly_when_pool_idles() {
    let user1 = Addr::unchecked("user1");
    let mut app = TerraApp::new(|router, _, storage| {
        // initialization moved to App construction
        router
            .bank
            .init_balance(
                storage,
                &user1,
                vec![coin(4666666_000000, "uusd"), coin(2000000_000000, "uluna")],
            )
            .unwrap()
    });

    // instantiate pair
    let pair_instance = instantiate_pair(&mut app, &user1);

    // provide liquidity, accumulators are empty
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(1000000_000000),
        Uint128::new(1000000_000000),
        None,
    );
    app.execute_contract(user1.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    const BLOCKS_PER_DAY: u64 = 17280;
    const ELAPSED_SECONDS: u64 = BLOCKS_PER_DAY * 5;

    // a day later
    app.update_block(|b| {
        b.height += BLOCKS_PER_DAY;
        b.time = b.time.plus_seconds(ELAPSED_SECONDS);
    });

    // provide liquidity, accumulators firstly filled with the same prices
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(3000000_000000),
        Uint128::new(1000000_000000),
        None,
    );
    app.execute_contract(user1.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    // get current twap accumulator values
    let msg = QueryMsg::CumulativePrices {};
    let cpr_old: CumulativePricesResponse =
        app.wrap().query_wasm_smart(&pair_instance, &msg).unwrap();

    // a day later
    app.update_block(|b| {
        b.height += BLOCKS_PER_DAY;
        b.time = b.time.plus_seconds(ELAPSED_SECONDS);
    });

    // get current twap accumulator values, it should be added up by the query method with new 2/1 ratio
    let msg = QueryMsg::CumulativePrices {};
    let cpr_new: CumulativePricesResponse =
        app.wrap().query_wasm_smart(&pair_instance, &msg).unwrap();

    let twap0 = cpr_new.price0_cumulative_last - cpr_old.price0_cumulative_last;
    let twap1 = cpr_new.price1_cumulative_last - cpr_old.price1_cumulative_last;

    // Prices weren't changed for the last day, uusd amount in pool = 4000000_000000, uluna = 2000000_000000
    let price_precision = Uint128::from(10u128.pow(TWAP_PRECISION.into()));
    assert_eq!(twap0 / price_precision, Uint128::new(85684)); // 1.008356286 * ELAPSED_SECONDS (86400)
    assert_eq!(twap1 / price_precision, Uint128::new(87121)); //   0.991712963 * ELAPSED_SECONDS
}

#[test]
fn create_pair_with_same_assets() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");

    let token_contract_code_id = store_token_code(&mut router);
    let pair_contract_code_id = store_pair_code(&mut router);

    let msg = InstantiateMsg {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: String::from("factory"),
        init_params: None,
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
fn update_pair_config() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");

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
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
        ],
        token_code_id: token_contract_code_id,
        factory_addr: factory_instance.to_string(),
        init_params: Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
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

    let params: StablePoolConfig = from_binary(&res.params.unwrap()).unwrap();

    assert_eq!(params.amp, Decimal::from_ratio(100u32, 1u32));

    //Start changing amp with incorrect next amp
    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StartChangingAmp {
            next_amp: MAX_AMP + 1,
            next_amp_time: router.block_info().time.seconds(),
        })
        .unwrap(),
    };

    let resp = router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        resp.root_cause().to_string(),
        format!(
            "Amp coefficient must be greater than 0 and less than or equal to {}",
            MAX_AMP
        )
    );

    //Start changing amp with big difference between the old and new amp value
    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StartChangingAmp {
            next_amp: 100 * MAX_AMP_CHANGE + 1,
            next_amp_time: router.block_info().time.seconds(),
        })
        .unwrap(),
    };

    let resp = router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        resp.root_cause().to_string(),
        format!(
            "The difference between the old and new amp value must not exceed {} times",
            MAX_AMP_CHANGE
        )
    );

    //Start changing amp earlier than the MIN_AMP_CHANGING_TIME has elapsed
    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StartChangingAmp {
            next_amp: 250,
            next_amp_time: router.block_info().time.seconds(),
        })
        .unwrap(),
    };

    let resp = router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        resp.root_cause().to_string(),
        format!(
            "Amp coefficient cannot be changed more often than once per {} seconds",
            MIN_AMP_CHANGING_TIME
        )
    );

    // Start increasing amp
    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME);
    });

    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StartChangingAmp {
            next_amp: 250,
            next_amp_time: router.block_info().time.seconds() + MIN_AMP_CHANGING_TIME,
        })
        .unwrap(),
    };

    router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap();

    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME / 2);
    });

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    let params: StablePoolConfig = from_binary(&res.params.unwrap()).unwrap();

    assert_eq!(params.amp, Decimal::from_ratio(175u32, 1u32));

    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME / 2);
    });

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    let params: StablePoolConfig = from_binary(&res.params.unwrap()).unwrap();

    assert_eq!(params.amp, Decimal::from_ratio(250u32, 1u32));

    // Start decreasing amp
    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME);
    });

    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StartChangingAmp {
            next_amp: 50,
            next_amp_time: router.block_info().time.seconds() + MIN_AMP_CHANGING_TIME,
        })
        .unwrap(),
    };

    router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap();

    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME / 2);
    });

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    let params: StablePoolConfig = from_binary(&res.params.unwrap()).unwrap();

    assert_eq!(params.amp, Decimal::from_ratio(150u32, 1u32));

    // Stop changing amp
    let msg = ExecuteMsg::UpdateConfig {
        params: to_binary(&StablePoolUpdateParams::StopChangingAmp {}).unwrap(),
    };

    router
        .execute_contract(owner.clone(), pair.clone(), &msg, &[])
        .unwrap();

    router.update_block(|b| {
        b.time = b.time.plus_seconds(MIN_AMP_CHANGING_TIME / 2);
    });

    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(pair.clone(), &QueryMsg::Config {})
        .unwrap();

    let params: StablePoolConfig = from_binary(&res.params.unwrap()).unwrap();

    assert_eq!(params.amp, Decimal::from_ratio(150u32, 1u32));
}
