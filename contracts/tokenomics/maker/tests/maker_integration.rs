#![cfg(not(tarpaulin_include))]

use std::str::FromStr;

use anyhow::Result as AnyResult;
use cosmwasm_std::{
    attr, coin, to_json_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Empty, Env,
    MessageInfo, QueryRequest, Response, StdResult, Uint128, Uint64, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;

use astroport::asset::{
    native_asset, native_asset_info, token_asset, token_asset_info, Asset, AssetInfo, AssetInfoExt,
    PairInfo,
};
use astroport::factory::{PairConfig, PairType, UpdateAddr};
use astroport::maker::{
    AssetWithLimit, BalancesResponse, ConfigResponse, DevFundConfig, ExecuteMsg, InstantiateMsg,
    QueryMsg, SecondReceiverConfig, SecondReceiverParams, SeizeConfig, UpdateDevFundConfig,
    COOLDOWN_LIMITS,
};
use astroport_maker::error::ContractError;
use astroport_test::cw_multi_test::{
    next_block, AppBuilder, AppResponse, BankSudo, Contract, ContractWrapper, Executor,
};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};

const OWNER: &str = "owner";

fn mock_app(owner: Addr, coins: Vec<Coin>) -> TestApp {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|router, _, storage| {
            // initialization moved to App construction
            router.bank.init_balance(storage, &owner, coins).unwrap();
        });

    app.update_block(|bi| {
        bi.height += 1;
        bi.chain_id = "cosm-wasm-test".to_string();
    });

    app
}

fn validate_and_send_funds(
    router: &mut TestApp,
    sender: &Addr,
    recipient: &Addr,
    funds: Vec<Coin>,
) {
    for fund in funds.clone() {
        // we cannot transfer zero coins
        if !fund.amount.is_zero() {
            router
                .send_tokens(sender.clone(), recipient.clone(), &[fund])
                .unwrap();
        }
    }
}

fn store_coin_registry_code(app: &mut TestApp) -> u64 {
    let coin_registry_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ));

    app.store_code(coin_registry_contract)
}

fn instantiate_coin_registry(mut app: &mut TestApp, coins: Option<Vec<(String, u8)>>) -> Addr {
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

fn mock_fee_distributor_contract() -> Box<dyn Contract<Empty>> {
    let instantiate = |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
        Ok(Default::default())
    };
    let execute = |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
        Ok(Default::default())
    };
    let empty_query = |_: Deps, _: Env, _: Empty| -> StdResult<Binary> { unimplemented!() };

    Box::new(ContractWrapper::new_with_empty(
        execute,
        instantiate,
        empty_query,
    ))
}

fn instantiate_contracts(
    mut router: &mut TestApp,
    owner: Addr,
    staking: Addr,
    governance_percent: Uint64,
    max_spread: Option<Decimal>,
    pair_type: Option<PairType>,
    second_receiver_params: Option<SecondReceiverParams>,
    collect_cooldown: Option<u64>,
) -> (Addr, Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
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

    let pair_code_id = match pair_type {
        Some(PairType::Stable {}) => {
            let pair_contract = Box::new(
                ContractWrapper::new_with_empty(
                    astroport_pair_stable::contract::execute,
                    astroport_pair_stable::contract::instantiate,
                    astroport_pair_stable::contract::query,
                )
                .with_reply_empty(astroport_pair_stable::contract::reply),
            );
            router.store_code(pair_contract)
        }
        _ => {
            let pair_contract = Box::new(
                ContractWrapper::new_with_empty(
                    astroport_pair::contract::execute,
                    astroport_pair::contract::instantiate,
                    astroport_pair::contract::query,
                )
                .with_reply_empty(astroport_pair::contract::reply),
            );
            router.store_code(pair_contract)
        }
    };

    let coin_registry_address = instantiate_coin_registry(
        &mut router,
        Some(vec![("uluna".to_string(), 6), ("uusd".to_string(), 6)]),
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
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            pair_type: pair_type.unwrap_or(PairType::Xyk {}),
            total_fee_bps: 0,
            maker_fee_bps: 0,
            is_disabled: false,
            is_generator_disabled: false,
            permissioned: false,
        }],
        token_code_id: 1u64,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
        coin_registry_address: coin_registry_address.to_string(),
        tracker_config: None,
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

    let escrow_fee_distributor_code_id = router.store_code(mock_fee_distributor_contract());

    let governance_instance = router
        .instantiate_contract(
            escrow_fee_distributor_code_id,
            owner.clone(),
            &Empty {},
            &[],
            "Astroport escrow fee distributor",
            None,
        )
        .unwrap();

    let maker_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_maker::contract::execute,
            astroport_maker::contract::instantiate,
            astroport_maker::contract::query,
        )
        .with_reply_empty(astroport_maker::reply::reply),
    );

    let market_code_id = router.store_code(maker_contract);

    let msg = InstantiateMsg {
        owner: String::from("owner"),
        factory_contract: factory_instance.to_string(),
        staking_contract: Some(staking.to_string()),
        governance_contract: Some(governance_instance.to_string()),
        governance_percent: Option::from(governance_percent),
        astro_token: token_asset_info(astro_token_instance.clone()),
        default_bridge: Some(native_asset_info("uluna".to_string())),
        max_spread,
        second_receiver_params,
        collect_cooldown,
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

    (
        astro_token_instance,
        factory_instance,
        maker_instance,
        governance_instance,
    )
}

fn instantiate_token(router: &mut TestApp, owner: Addr, name: String, symbol: String) -> Addr {
    let token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
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
    router: &mut TestApp,
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

fn allowance_token(router: &mut TestApp, owner: Addr, spender: Addr, token: Addr, amount: Uint128) {
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

fn check_balance(router: &mut TestApp, user: Addr, token: Addr, expected_amount: Uint128) {
    let msg = Cw20QueryMsg::Balance {
        address: user.to_string(),
    };

    let res: Result<BalanceResponse, _> =
        router.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token.to_string(),
            msg: to_json_binary(&msg).unwrap(),
        }));

    let balance = res.unwrap();

    assert_eq!(balance.balance, expected_amount);
}

fn create_pair(
    mut router: &mut TestApp,
    owner: Addr,
    user: Addr,
    factory_instance: &Addr,
    assets: Vec<Asset>,
    pair_type: Option<PairType>,
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

    let asset_infos = assets.iter().cloned().map(|a| a.info).collect::<Vec<_>>();

    // Create pair in factory
    let res = router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: pair_type.unwrap_or(PairType::Xyk {}),
                asset_infos: asset_infos.clone(),
                init_params: None,
            },
            &[],
        )
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));

    // Get pair
    let pair_info: PairInfo = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_json_binary(&astroport::factory::QueryMsg::Pair {
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

    funds.sort_by(|l, r| l.denom.cmp(&r.denom));

    let user_funds: Vec<Coin> = funds
        .iter()
        .map(|c| Coin {
            denom: c.denom.clone(),
            amount: c.amount * Uint128::new(2),
        })
        .collect();

    validate_and_send_funds(router, &owner, &user, user_funds);

    router
        .execute_contract(
            user.clone(),
            pair_info.contract_addr.clone(),
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets: vec![assets[0].clone(), assets[1].clone()],
                slippage_tolerance: None,
                auto_stake: None,
                receiver: None,
                min_lp_to_receive: None,
            },
            &funds,
        )
        .unwrap();

    pair_info
}

#[test]
fn update_config() {
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
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &mut router,
            owner.clone(),
            staking.clone(),
            governance_percent,
            None,
            None,
            None,
            None,
        );

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &msg)
        .unwrap();

    assert_eq!(res.owner, owner);
    assert_eq!(res.astro_token, token_asset_info(astro_token_instance));
    assert_eq!(res.factory_contract, factory_instance);
    assert_eq!(res.staking_contract, Some(staking));
    assert_eq!(res.governance_contract, Some(governance_instance));
    assert_eq!(res.governance_percent, governance_percent);
    assert_eq!(res.max_spread, Decimal::from_str("0.05").unwrap());

    let new_staking = Addr::unchecked("new_staking");
    let new_factory = Addr::unchecked("new_factory");
    let new_governance = Addr::unchecked("new_governance");
    let new_governance_percent = Uint64::new(50);
    let new_max_spread = Decimal::from_str("0.5").unwrap();

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: Some(new_governance_percent),
        governance_contract: Some(UpdateAddr::Set(new_governance.to_string())),
        staking_contract: Some(new_staking.to_string()),
        factory_contract: Some(new_factory.to_string()),
        basic_asset: None,
        max_spread: Some(new_max_spread),
        second_receiver_params: None,
        collect_cooldown: None,
        astro_token: None,
        dev_fund_config: None,
    };

    // Assert cannot update with improper owner
    let e = router
        .execute_contract(
            Addr::unchecked("not_owner"),
            maker_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();

    assert_eq!(e.root_cause().to_string(), "Unauthorized");

    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &msg)
        .unwrap();

    assert_eq!(res.factory_contract, new_factory);
    assert_eq!(res.staking_contract, Some(new_staking));
    assert_eq!(res.governance_percent, new_governance_percent);
    assert_eq!(res.governance_contract, Some(new_governance.clone()));
    assert_eq!(res.max_spread, new_max_spread);

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: Some(UpdateAddr::Remove {}),
        staking_contract: None,
        factory_contract: None,
        basic_asset: None,
        max_spread: None,
        second_receiver_params: Some(SecondReceiverParams {
            second_fee_receiver: "second_fee_receiver".to_string(),
            second_receiver_cut: Default::default(),
        }),
        collect_cooldown: None,
        astro_token: None,
        dev_fund_config: None,
    };

    let err = router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!("Generic error: Incorrect second receiver percent of its share. Should be in range: 0 < 0 <= 50", err.root_cause().to_string());

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: Some(UpdateAddr::Remove {}),
        staking_contract: None,
        factory_contract: None,
        basic_asset: None,
        max_spread: None,
        second_receiver_params: Some(SecondReceiverParams {
            second_fee_receiver: "second_fee_receiver".to_string(),
            second_receiver_cut: Uint64::new(10),
        }),
        collect_cooldown: None,
        astro_token: None,
        dev_fund_config: None,
    };

    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &msg)
        .unwrap();
    assert_eq!(res.governance_contract, None);
    assert_eq!(
        res.second_receiver_cfg,
        Some(SecondReceiverConfig {
            second_fee_receiver: Addr::unchecked("second_fee_receiver"),
            second_receiver_cut: Uint64::new(10)
        })
    );

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: None,
        staking_contract: None,
        factory_contract: None,
        basic_asset: None,
        max_spread: None,
        second_receiver_params: None,
        collect_cooldown: Some(*COOLDOWN_LIMITS.start() - 1),
        astro_token: None,
        dev_fund_config: None,
    };

    let err = router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IncorrectCooldown {
            min: *COOLDOWN_LIMITS.start(),
            max: *COOLDOWN_LIMITS.end()
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: None,
        staking_contract: None,
        factory_contract: None,
        basic_asset: None,
        max_spread: None,
        second_receiver_params: None,
        collect_cooldown: Some(*COOLDOWN_LIMITS.end() + 1),
        astro_token: None,
        dev_fund_config: None,
    };
    let err = router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IncorrectCooldown {
            min: *COOLDOWN_LIMITS.start(),
            max: *COOLDOWN_LIMITS.end()
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: None,
        staking_contract: None,
        factory_contract: None,
        basic_asset: None,
        max_spread: None,
        second_receiver_params: None,
        collect_cooldown: Some((*COOLDOWN_LIMITS.end() - *COOLDOWN_LIMITS.start()) / 2),
        astro_token: None,
        dev_fund_config: None,
    };
    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();
}

fn test_maker_collect(
    mut router: TestApp,
    owner: Addr,
    factory_instance: Addr,
    maker_instance: Addr,
    staking: Addr,
    governance: Addr,
    governance_percent: Uint64,
    pairs: Vec<Vec<Asset>>,
    assets: Vec<AssetWithLimit>,
    bridges: Vec<(AssetInfo, AssetInfo)>,
    mint_balances: Vec<(Addr, u128)>,
    native_balances: Vec<Coin>,
    expected_balances: Vec<Asset>,
    collected_balances: Vec<(Addr, u128)>,
) {
    let user = Addr::unchecked("user0000");

    // Create pairs
    for t in pairs {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );
    }

    // Setup bridge to withdraw USDC via USDC -> TEST -> UUSD -> ASTRO route
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::UpdateBridges {
                add: Some(bridges),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // enable rewards distribution
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap();

    // Mint all tokens for maker
    for t in mint_balances {
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

    validate_and_send_funds(&mut router, &owner, &maker_instance, native_balances);

    let balances_resp: BalancesResponse = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Balances {
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
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect { assets },
            &[],
        )
        .unwrap();

    for t in collected_balances {
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
fn collect_all() {
    let owner = Addr::unchecked("owner");
    let uluna_asset = "uluna".to_string();

    let mut router = mock_app(
        owner.clone(),
        vec![Coin {
            denom: uluna_asset.clone(),
            amount: Uint128::new(100_000_000_000u128),
        }],
    );
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(0);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        governance_percent,
        Some(max_spread),
        None,
        None,
        None,
    );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    // Create pairs
    let pairs = vec![
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ];

    // Specify assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uluna_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    let bridges = vec![
        (
            token_asset_info(test_token_instance.clone()),
            token_asset_info(bridge2_token_instance.clone()),
        ),
        (
            token_asset_info(usdc_token_instance.clone()),
            token_asset_info(test_token_instance.clone()),
        ),
        (
            native_asset_info(uluna_asset.to_string()),
            token_asset_info(test_token_instance.clone()),
        ),
    ];

    let mint_balances = vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token_instance.clone(), 30u128),
    ];

    let native_balances = vec![Coin {
        denom: uluna_asset.clone(),
        amount: Uint128::new(100),
    }];

    let expected_balances = vec![
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        native_asset(uluna_asset.clone(), Uint128::new(100)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token_instance.clone(), Uint128::new(30)),
    ];

    let collected_balances = vec![
        // 154 ASTRO = 10 ASTRO +
        // 98 ASTRO (100 uluna -> 100 usdc - 1 fee -> 109 bridge - 1 fee) +
        // 18 ASTRO (20 usdc -> 20 test - 1 fee -> 19 bridge - 1 fee) +
        // 28 ASTRO (30 test -> 30 bridge - 1 fee -> 29 - 1 fee)
        (astro_token_instance.clone(), 154u128),
        (usdc_token_instance.clone(), 0u128),
        (test_token_instance.clone(), 0u128),
    ];

    test_maker_collect(
        router,
        owner,
        factory_instance,
        maker_instance,
        staking,
        Addr::unchecked("governance"),
        governance_percent,
        pairs,
        assets,
        bridges,
        mint_balances,
        native_balances,
        expected_balances,
        collected_balances,
    );
}

#[test]
fn collect_maxdepth_test() {
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
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        governance_percent,
        Some(max_spread),
        None,
        None,
        None,
    );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    // Create pairs
    let mut pair_addresses = vec![];
    for t in vec![
        vec![
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        let pair_info = create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );

        pair_addresses.push(pair_info.contract_addr);
    }

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    let err = router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::UpdateBridges {
                add: Some(vec![
                    (
                        token_asset_info(test_token_instance.clone()),
                        token_asset_info(bridge2_token_instance.clone()),
                    ),
                    (
                        token_asset_info(usdc_token_instance.clone()),
                        token_asset_info(test_token_instance.clone()),
                    ),
                    (
                        native_asset_info(uluna_asset.clone()),
                        token_asset_info(usdc_token_instance.clone()),
                    ),
                    (
                        native_asset_info(uusd_asset.clone()),
                        native_asset_info(uluna_asset.clone()),
                    ),
                ]),
                remove: None,
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Max bridge length of 2 was reached"
    )
}

#[test]
fn collect_err_no_swap_pair() {
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uabc".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "ukrt".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(50);

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        governance_percent,
        None,
        None,
        None,
        None,
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");
    let ukrt_asset = String::from("ukrt");
    let uabc_asset = String::from("uabc");

    // Mint all tokens for Maker
    for t in vec![
        vec![
            native_asset(ukrt_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(ukrt_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uabc_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );
    }

    // Set the assets to swap
    let assets = vec![
        AssetWithLimit {
            info: native_asset(ukrt_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uabc_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Mint all tokens for the Maker
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
        .send_tokens(
            owner.clone(),
            maker_instance.clone(),
            &[coin(20, ukrt_asset.clone()), coin(30, uabc_asset.clone())],
        )
        .unwrap();

    let msg = ExecuteMsg::Collect { assets };

    let e = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        e.root_cause().to_string(),
        "Cannot swap uabc. No swap destinations",
    );
}

#[test]
fn update_bridges() {
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
            Coin {
                denom: "ukrt".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);
    let user = Addr::unchecked("user0000");
    let uusd_asset = String::from("uusd");

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        governance_percent,
        None,
        None,
        None,
        None,
    );

    let msg = ExecuteMsg::UpdateBridges {
        add: Some(vec![
            (
                native_asset_info(String::from("uluna")),
                native_asset_info(String::from("uusd")),
            ),
            (
                native_asset_info(String::from("ukrt")),
                native_asset_info(String::from("uusd")),
            ),
        ]),
        remove: None,
    };

    // Unauthorized check
    let err = router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // Add bridges
    let err = router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Invalid bridge. Pool uluna to uusd not found"
    );

    // Create pair so that add bridge check does not fail
    for pair in vec![
        vec![
            native_asset(String::from("uluna"), Uint128::from(100_000_u128)),
            native_asset(String::from("uusd"), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(String::from("ukrt"), Uint128::from(100_000_u128)),
            native_asset(String::from("uusd"), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            pair,
            None,
        );
    }

    // Add bridges
    let err = router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Invalid bridge destination. uluna cannot be swapped to ASTRO"
    );

    // Create pair so that add bridge check does not fail
    create_pair(
        &mut router,
        owner.clone(),
        user.clone(),
        &factory_instance,
        vec![
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        None,
    );

    // Add bridges
    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let resp: Vec<(String, String)> = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Bridges {}).unwrap(),
        }))
        .unwrap();

    assert_eq!(
        resp,
        vec![
            (String::from("ukrt"), String::from("uusd")),
            (String::from("uluna"), String::from("uusd")),
        ]
    );

    let msg = ExecuteMsg::UpdateBridges {
        remove: Some(vec![native_asset_info(String::from("ukrt"))]),
        add: None,
    };

    // Remove bridges
    router
        .execute_contract(owner.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    let resp: Vec<(String, String)> = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Bridges {}).unwrap(),
        }))
        .unwrap();

    assert_eq!(resp, vec![(String::from("uluna"), String::from("uusd"))]);
}

#[test]
fn collect_with_asset_limit() {
    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: uusd_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: uluna_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &mut router,
            owner.clone(),
            staking.clone(),
            governance_percent,
            Some(max_spread),
            None,
            None,
            None,
        );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    // Create pairs
    for t in vec![
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset, Uint128::from(100_000_u128)),
            native_asset(uusd_asset, Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );
    }

    // Make a list with duplicate assets
    let assets_with_duplicate = vec![
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
    ];

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::UpdateBridges {
                add: Some(vec![
                    (
                        token_asset_info(test_token_instance.clone()),
                        token_asset_info(bridge2_token_instance.clone()),
                    ),
                    (
                        token_asset_info(usdc_token_instance.clone()),
                        token_asset_info(test_token_instance.clone()),
                    ),
                ]),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // Enable rewards distribution
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token_instance.clone(), 30u128),
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

    let expected_balances = vec![
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token_instance.clone(), Uint128::new(30)),
    ];

    let balances_resp: BalancesResponse = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Balances {
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

    let resp = router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets_with_duplicate.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        resp.root_cause().to_string(),
        "Cannot collect. Remove duplicate asset",
    );

    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // Check Maker's balance of ASTRO tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    // Check Maker's balance of USDC tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::new(15u128),
    );

    // Check Maker's balance of test tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        test_token_instance.clone(),
        Uint128::new(0u128),
    );

    // Check balances
    // We are losing 1 ASTRO in fees per swap
    // 40 ASTRO = 10 astro +
    // 2 usdc (5 - fee for 3 swaps)
    // 28 test (30 - fee for 2 swaps)
    let amount = Uint128::new(40u128);
    let governance_amount =
        amount.multiply_ratio(Uint128::from(governance_percent), Uint128::new(100));
    let staking_amount = amount - governance_amount;

    // Check the governance contract's balance for the ASTRO token
    check_balance(
        &mut router,
        governance_instance.clone(),
        astro_token_instance.clone(),
        governance_amount,
    );

    // Check the governance contract's balance for the USDC token
    check_balance(
        &mut router,
        governance_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the governance contract's balance for the test token
    check_balance(
        &mut router,
        governance_instance.clone(),
        test_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the ASTRO token
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        staking_amount,
    );

    // Check the staking contract's balance for the USDC token
    check_balance(
        &mut router,
        staking.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the test token
    check_balance(
        &mut router,
        staking.clone(),
        test_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn collect_with_second_receiver() {
    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: uusd_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: uluna_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &mut router,
            owner.clone(),
            staking.clone(),
            governance_percent,
            Some(max_spread),
            None,
            Some(SecondReceiverParams {
                second_fee_receiver: "second_receiver".to_string(),
                second_receiver_cut: Uint64::new(50),
            }),
            None,
        );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    // Create pairs
    for t in vec![
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset, Uint128::from(100_000_u128)),
            native_asset(uusd_asset, Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );
    }

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
    ];

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::UpdateBridges {
                add: Some(vec![
                    (
                        token_asset_info(test_token_instance.clone()),
                        token_asset_info(bridge2_token_instance.clone()),
                    ),
                    (
                        token_asset_info(usdc_token_instance.clone()),
                        token_asset_info(test_token_instance.clone()),
                    ),
                ]),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // Enable rewards distribution
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token_instance.clone(), 30u128),
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

    let expected_balances = vec![
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token_instance.clone(), Uint128::new(30)),
    ];

    let balances_resp: BalancesResponse = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Balances {
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
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // Check Maker's balance of ASTRO tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    // Check Maker's balance of USDC tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::new(15u128),
    );

    // Check Maker's balance of test tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        test_token_instance.clone(),
        Uint128::new(0u128),
    );

    // Check balances
    let amount = Uint128::new(40u128);
    let second_receiver_amount = amount.multiply_ratio(Uint128::from(50u64), Uint128::new(100));
    let governance_amount = amount
        .checked_sub(second_receiver_amount)
        .unwrap()
        .multiply_ratio(Uint128::from(governance_percent), Uint128::new(100));
    let staking_amount = amount - governance_amount - second_receiver_amount;

    // Check the second receiver contract's balance for the ASTRO token
    check_balance(
        &mut router,
        Addr::unchecked("second_receiver"),
        astro_token_instance.clone(),
        second_receiver_amount,
    );

    // Check the governance contract's balance for the ASTRO token
    check_balance(
        &mut router,
        governance_instance.clone(),
        astro_token_instance.clone(),
        governance_amount,
    );

    // Check the governance contract's balance for the USDC token
    check_balance(
        &mut router,
        governance_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the governance contract's balance for the test token
    check_balance(
        &mut router,
        governance_instance.clone(),
        test_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the ASTRO token
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        staking_amount,
    );

    // Check the staking contract's balance for the USDC token
    check_balance(
        &mut router,
        staking.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the test token
    check_balance(
        &mut router,
        staking.clone(),
        test_token_instance.clone(),
        Uint128::zero(),
    );
}

#[test]
fn test_collect_cooldown() {
    let asset0 = "asset0";
    let asset1 = "asset1";
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            coin(100_000_000_000u128, asset0),
            coin(100_000_000_000u128, asset1),
        ],
    );

    let (_, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        Addr::unchecked("staking"),
        10u64.into(),
        Some(Decimal::from_str("0.5").unwrap()),
        None,
        None,
        Some(300),
    );

    // Moving block time to be able to collect
    router.update_block(|block| block.time = block.time.plus_seconds(300));

    let asset_infos = [AssetInfo::native(asset0), AssetInfo::native(asset1)];

    // Create pair in factory
    router
        .execute_contract(
            owner.clone(),
            factory_instance.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Xyk {},
                asset_infos: asset_infos.to_vec(),
                init_params: None,
            },
            &[],
        )
        .unwrap();

    // Set assets to swap
    let assets = vec![AssetWithLimit {
        info: AssetInfo::native(asset0),
        limit: None,
    }];

    // First collect works
    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();
    let next_collect_ts = router.block_info().time.plus_seconds(300).seconds();

    // Cooldown is 300 sec. We can't collect again
    router.update_block(|block| block.time = block.time.plus_seconds(100));

    let err = router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Cooldown { next_collect_ts }
    );

    // In 200 seconds cooldown should be expired
    router.update_block(|block| block.time = block.time.plus_seconds(200));
    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();
}

fn set_dev_fund_config(
    app: &mut TestApp,
    sender: &Addr,
    maker: &Addr,
    dev_fund_config: UpdateDevFundConfig,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        sender.clone(),
        maker.clone(),
        &ExecuteMsg::UpdateConfig {
            factory_contract: None,
            staking_contract: None,
            governance_contract: None,
            governance_percent: None,
            basic_asset: None,
            max_spread: None,
            second_receiver_params: None,
            collect_cooldown: None,
            astro_token: None,
            dev_fund_config: Some(Box::new(dev_fund_config)),
        },
        &[],
    )
}

fn mint_coins(app: &mut TestApp, to: impl Into<String>, amount: &[Coin]) {
    app.sudo(
        BankSudo::Mint {
            to_address: to.into(),
            amount: amount.to_vec(),
        }
        .into(),
    )
    .unwrap();
}

#[test]
fn test_dev_fund_fee() {
    let usdc = "uusdc";
    let owner = Addr::unchecked("owner");
    let mut app = mock_app(owner.clone(), vec![coin(300_000_000_000u128, usdc)]);

    let staking = Addr::unchecked("staking");
    let (astro_token, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut app,
        owner.clone(),
        staking.clone(),
        0u64.into(),
        Some(Decimal::from_str("0.5").unwrap()),
        None,
        None,
        None,
    );

    // enable rewards
    app.execute_contract(
        owner.clone(),
        maker_instance.clone(),
        &ExecuteMsg::EnableRewards { blocks: 1 },
        &[],
    )
    .unwrap();

    let mut dev_fund_conf = DevFundConfig {
        address: "".to_string(),
        share: Default::default(),
        asset_info: AssetInfo::native(usdc),
    };

    let err = set_dev_fund_config(
        &mut app,
        &owner,
        &maker_instance,
        UpdateDevFundConfig {
            set: Some(dev_fund_conf.clone()),
        },
    )
    .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Invalid input: human address too short for this mock implementation (must be >= 3)."
    );

    dev_fund_conf.address = "devs".to_string();

    let err = set_dev_fund_config(
        &mut app,
        &owner,
        &maker_instance,
        UpdateDevFundConfig {
            set: Some(dev_fund_conf.clone()),
        },
    )
    .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Dev fund share must be > 0 and <= 1"
    );

    dev_fund_conf.share = Decimal::percent(50);

    let err = set_dev_fund_config(
        &mut app,
        &owner,
        &maker_instance,
        UpdateDevFundConfig {
            set: Some(dev_fund_conf.clone()),
        },
    )
    .unwrap_err();
    assert_eq!(
        ContractError::InvalidBridgeNoPool(astro_token.to_string(), usdc.to_string()),
        err.downcast().unwrap()
    );

    // Create ASTRO<>USDC pool
    create_pair(
        &mut app,
        owner.clone(),
        owner.clone(),
        &factory_instance,
        vec![
            AssetInfo::native(usdc).with_balance(100_000_000000u128),
            AssetInfo::cw20(astro_token.clone()).with_balance(100_000_000000u128),
        ],
        None,
    );

    set_dev_fund_config(
        &mut app,
        &owner,
        &maker_instance,
        UpdateDevFundConfig {
            set: Some(dev_fund_conf.clone()),
        },
    )
    .unwrap();

    // Emulate usdc income to the Maker contract
    mint_coins(
        &mut app,
        maker_instance.to_string(),
        &[coin(1000_000000u128, usdc)],
    );

    app.execute_contract(
        Addr::unchecked("anyone"),
        maker_instance.clone(),
        &ExecuteMsg::Collect {
            assets: vec![AssetWithLimit {
                info: AssetInfo::native(usdc),
                limit: None,
            }],
        },
        &[],
    )
    .unwrap();

    // Check balances
    // ASTRO
    check_balance(
        &mut app,
        maker_instance.clone(),
        astro_token.clone(),
        0u128.into(),
    );
    check_balance(
        &mut app,
        staking.clone(),
        astro_token.clone(),
        495_049505u128.into(),
    );
    check_balance(
        &mut app,
        Addr::unchecked(&dev_fund_conf.address),
        astro_token.clone(),
        0u128.into(),
    );
    // USDC
    assert_eq!(
        app.wrap()
            .query_balance(&maker_instance, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        app.wrap()
            .query_balance(&dev_fund_conf.address, usdc)
            .unwrap()
            .amount
            .u128(),
        502_487561
    );

    // Disable dev funds
    set_dev_fund_config(
        &mut app,
        &owner,
        &maker_instance,
        UpdateDevFundConfig { set: None },
    )
    .unwrap();

    // Emulate usdc income to the Maker contract
    mint_coins(
        &mut app,
        maker_instance.to_string(),
        &[coin(1000_000000u128, usdc)],
    );

    app.execute_contract(
        Addr::unchecked("anyone"),
        maker_instance.clone(),
        &ExecuteMsg::Collect {
            assets: vec![AssetWithLimit {
                info: AssetInfo::native(usdc),
                limit: None,
            }],
        },
        &[],
    )
    .unwrap();

    // Check balances
    // ASTRO
    check_balance(
        &mut app,
        maker_instance.clone(),
        astro_token.clone(),
        0u128.into(),
    );
    check_balance(
        &mut app,
        staking.clone(),
        astro_token.clone(),
        1475_417871u128.into(),
    );
    check_balance(
        &mut app,
        Addr::unchecked(&dev_fund_conf.address),
        astro_token.clone(),
        0u128.into(),
    );
    // USDC
    assert_eq!(
        app.wrap()
            .query_balance(&maker_instance, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        app.wrap()
            .query_balance(&dev_fund_conf.address, usdc)
            .unwrap()
            .amount
            .u128(),
        502_487561
    );
}

#[test]
fn test_seize() {
    let owner = Addr::unchecked("owner");
    let mut app = mock_app(owner.clone(), vec![]);

    let (_, _, maker_instance, _) = instantiate_contracts(
        &mut app,
        owner.clone(),
        Addr::unchecked("staking"),
        0u64.into(),
        Some(Decimal::from_str("0.5").unwrap()),
        None,
        None,
        None,
    );

    // Try to seize before config is set
    let err = app
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::Seize { assets: vec![] },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: assets vector is empty"
    );

    // Unauthorized check
    let err = app
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::UpdateSeizeConfig {
                receiver: None,
                seizable_assets: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let receiver = Addr::unchecked("seize");

    let usdc = "uusdc";
    let luna = "uluna";

    // Set valid config
    app.execute_contract(
        owner.clone(),
        maker_instance.clone(),
        &ExecuteMsg::UpdateSeizeConfig {
            receiver: Some(receiver.to_string()),
            seizable_assets: vec![AssetInfo::native(usdc), AssetInfo::native(luna)],
        },
        &[],
    )
    .unwrap();

    // Assert that the config is set
    let config: SeizeConfig = app
        .wrap()
        .query_wasm_smart(&maker_instance, &QueryMsg::QuerySeizeConfig {})
        .unwrap();
    assert_eq!(
        config,
        SeizeConfig {
            receiver: receiver.clone(),
            seizable_assets: vec![AssetInfo::native(usdc), AssetInfo::native(luna)]
        }
    );

    // Try to seize non-seizable asset
    let err = app
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::Seize {
                assets: vec![AssetWithLimit {
                    info: AssetInfo::native("utest"),
                    limit: None,
                }],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Input vector contains assets that are not seizable"
    );

    // Try to seize asset with empty balance
    // This does nothing and doesn't throw an error
    app.execute_contract(
        owner.clone(),
        maker_instance.clone(),
        &ExecuteMsg::Seize {
            assets: vec![AssetWithLimit {
                info: AssetInfo::native(luna),
                limit: None,
            }],
        },
        &[],
    )
    .unwrap();

    mint_coins(
        &mut app,
        &maker_instance,
        &[coin(1000_000000u128, usdc), coin(3000_000000u128, luna)],
    );

    // Seize 100 USDC
    app.execute_contract(
        owner.clone(),
        maker_instance.clone(),
        &ExecuteMsg::Seize {
            assets: vec![AssetWithLimit {
                info: AssetInfo::native(usdc),
                limit: Some(100_000000u128.into()),
            }],
        },
        &[],
    )
    .unwrap();

    // Check balances
    assert_eq!(
        app.wrap()
            .query_balance(&maker_instance, usdc)
            .unwrap()
            .amount
            .u128(),
        900_000000
    );
    assert_eq!(
        app.wrap()
            .query_balance(&receiver, usdc)
            .unwrap()
            .amount
            .u128(),
        100_000000
    );

    // Seize all
    app.execute_contract(
        owner.clone(),
        maker_instance.clone(),
        &ExecuteMsg::Seize {
            assets: vec![
                AssetWithLimit {
                    info: AssetInfo::native(usdc),
                    // seizing more than available doesn't throw an error
                    limit: Some(10000_000000u128.into()),
                },
                AssetWithLimit {
                    info: AssetInfo::native(luna),
                    limit: Some(3000_000000u128.into()),
                },
            ],
        },
        &[],
    )
    .unwrap();

    // Check balances
    assert_eq!(
        app.wrap()
            .query_balance(&maker_instance, usdc)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        app.wrap()
            .query_balance(&maker_instance, luna)
            .unwrap()
            .amount
            .u128(),
        0
    );
    assert_eq!(
        app.wrap()
            .query_balance(&receiver, usdc)
            .unwrap()
            .amount
            .u128(),
        1000_000000
    );
    assert_eq!(
        app.wrap()
            .query_balance(&receiver, luna)
            .unwrap()
            .amount
            .u128(),
        3000_000000
    );
}

struct CheckDistributedAstro {
    maker_amount: Uint128,
    governance_amount: Uint128,
    staking_amount: Uint128,
    governance_percent: Uint64,
    maker: Addr,
    astro_token: Addr,
    governance: Addr,
    staking: Addr,
}

impl CheckDistributedAstro {
    fn check(&mut self, router: &mut TestApp, distributed_amount: u32) {
        let distributed_amount = Uint128::from(distributed_amount as u128);
        let cur_governance_amount = distributed_amount
            .multiply_ratio(Uint128::from(self.governance_percent), Uint128::new(100));
        self.governance_amount += cur_governance_amount;
        self.staking_amount += distributed_amount - cur_governance_amount;
        self.maker_amount -= distributed_amount;

        check_balance(
            router,
            self.maker.clone(),
            self.astro_token.clone(),
            self.maker_amount,
        );

        check_balance(
            router,
            self.governance.clone(),
            self.astro_token.clone(),
            self.governance_amount,
        );

        check_balance(
            router,
            self.staking.clone(),
            self.astro_token.clone(),
            self.staking_amount,
        );
    }
}

#[test]
fn distribute_initially_accrued_fees() {
    let uluna_asset = String::from("uluna");
    let owner = Addr::unchecked("owner");

    let mut router = mock_app(
        owner.clone(),
        vec![Coin {
            denom: uluna_asset.clone(),
            amount: Uint128::new(100_000_000_000_000000u128),
        }],
    );

    let staking = Addr::unchecked("staking");
    let governance_percent = Uint64::new(10);
    let user = Addr::unchecked("user0000");

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &mut router,
            owner.clone(),
            staking.clone(),
            governance_percent,
            None,
            None,
            None,
            None,
        );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    // Create pairs
    for t in vec![
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            None,
        );
    }

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uluna_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Setup bridge to withdraw USDC via the USDC -> TEST -> ASTRO route
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::UpdateBridges {
                add: Some(vec![
                    (
                        token_asset_info(test_token_instance.clone()),
                        token_asset_info(bridge2_token_instance.clone()),
                    ),
                    (
                        token_asset_info(usdc_token_instance.clone()),
                        token_asset_info(test_token_instance.clone()),
                    ),
                    (
                        native_asset_info(uluna_asset.clone()),
                        token_asset_info(usdc_token_instance.clone()),
                    ),
                ]),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance, 20u128),
        (test_token_instance, 30u128),
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
        .send_tokens(
            owner.clone(),
            maker_instance.clone(),
            &[coin(100, uluna_asset.clone())],
        )
        .unwrap();

    // Unauthorized check
    let err = router
        .execute_contract(
            user.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // Check pre_update_blocks = 0
    let err = router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 0 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Number of blocks should be > 0"
    );

    // Check that collect does not distribute ASTRO until rewards are enabled
    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect { assets },
            &[],
        )
        .unwrap();

    // Balances checker
    let mut checker = CheckDistributedAstro {
        maker_amount: Uint128::new(151_u128),
        governance_amount: Uint128::zero(),
        staking_amount: Uint128::zero(),
        maker: maker_instance.clone(),
        astro_token: astro_token_instance.clone(),
        governance_percent,
        governance: governance_instance,
        staking,
    };
    checker.check(&mut router, 0);

    // Enable rewards distribution
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 10 },
            &[],
        )
        .unwrap();

    // Try to enable again
    let err = router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Rewards collecting is already enabled"
    );

    let astro_asset = AssetWithLimit {
        info: token_asset_info(astro_token_instance.clone()),
        limit: None,
    };
    let assets = vec![astro_asset];

    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // Since the block number is the same, nothing happened
    checker.check(&mut router, 0);

    router.update_block(next_block);

    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    checker.check(&mut router, 15);

    // Let's try to collect again within the same block
    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // But no ASTRO were distributed
    checker.check(&mut router, 0);

    router.update_block(next_block);

    // Imagine that we received new fees the while pre-ugrade ASTRO is being distributed
    mint_some_token(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        maker_instance.clone(),
        Uint128::from(30_u128),
    );

    let resp = router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    checker.maker_amount += Uint128::from(30_u128);
    // 45 = 30 minted astro + 15 distributed astro
    checker.check(&mut router, 45);

    // Checking that attributes are set properly
    for (attr, value) in [
        ("astro_distribution", 30_u128),
        ("preupgrade_astro_distribution", 15_u128),
    ] {
        let a = resp.events[1]
            .attributes
            .iter()
            .find(|a| a.key == attr)
            .unwrap();
        assert_eq!(a.value, value.to_string());
    }

    // Increment 8 blocks
    for _ in 0..8 {
        router.update_block(next_block);
    }

    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // 120 = 15 * 8
    checker.check(&mut router, 120);

    // Check remainder reward
    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.remainder_reward.u128(), 1_u128);

    // Check remainder reward distribution
    router.update_block(next_block);

    router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    checker.check(&mut router, 1);

    // Check that the pre-upgrade ASTRO was fully distributed
    let res: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&maker_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.remainder_reward.u128(), 0_u128);
    assert_eq!(res.pre_upgrade_astro_amount.u128(), 151_u128);

    // Check usual collecting works
    mint_some_token(
        &mut router,
        owner,
        astro_token_instance,
        maker_instance.clone(),
        Uint128::from(115_u128),
    );

    let resp = router
        .execute_contract(
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect { assets },
            &[],
        )
        .unwrap();

    checker.maker_amount += Uint128::from(115_u128);
    checker.check(&mut router, 115);

    // Check that attributes are set properly
    let a = resp.events[1]
        .attributes
        .iter()
        .find(|a| a.key == "astro_distribution")
        .unwrap();
    assert_eq!(a.value, 115_u128.to_string());
    assert!(!resp.events[1]
        .attributes
        .iter()
        .any(|a| a.key == "preupgrade_astro_distribution"));
}

#[ignore]
#[test]
fn collect_3pools() {
    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");
    let owner = Addr::unchecked("owner");
    let mut router = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: uusd_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: uluna_asset.clone(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );
    let user = Addr::unchecked("user0000");
    let staking = Addr::unchecked("staking");
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &mut router,
        owner.clone(),
        staking.clone(),
        Default::default(),
        Some(max_spread),
        Some(PairType::Stable {}),
        None,
        None,
    );

    let usdc_token_instance = instantiate_token(
        &mut router,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token = instantiate_token(
        &mut router,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    // Create pairs
    // There are 2 routes to swap USDC -> LUNA: through (USDC, TEST, LUNA) or (USDC, LUNA)
    for t in vec![
        vec![
            // intentionally providing less usdc thus this pool will be selected to swap USDC -> LUNA
            token_asset(usdc_token_instance.clone(), Uint128::from(80_000_u128)),
            token_asset(test_token.clone(), Uint128::from(100_000_u128)),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        vec![
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            t,
            Some(PairType::Stable {}),
        );
    }

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(test_token.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Enable rewards distribution
    router
        .execute_contract(
            owner.clone(),
            maker_instance.clone(),
            &ExecuteMsg::EnableRewards { blocks: 1 },
            &[],
        )
        .unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token.clone(), 30u128),
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

    let expected_balances = vec![
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token.clone(), Uint128::new(30)),
    ];

    let balances_resp: BalancesResponse = router
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: maker_instance.to_string(),
            msg: to_json_binary(&QueryMsg::Balances {
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
            Addr::unchecked("anyone"),
            maker_instance.clone(),
            &ExecuteMsg::Collect {
                assets: assets.clone(),
            },
            &[],
        )
        .unwrap();

    // Check Maker's balance of ASTRO tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    // Check Maker's balance of USDC tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check Maker's balance of test tokens
    check_balance(
        &mut router,
        maker_instance.clone(),
        test_token.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the ASTRO token
    check_balance(
        &mut router,
        staking.clone(),
        astro_token_instance.clone(),
        Uint128::new(62u128),
    );

    // Check that USDC -> LUNA swap was not executed through pair (usdc, luna) but through (usdc, luna, test).
    let pair_info: PairInfo = router
        .wrap()
        .query_wasm_smart(
            &factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: vec![
                    token_asset_info(usdc_token_instance),
                    native_asset_info(uluna_asset),
                ],
            },
        )
        .unwrap();
    let balances = pair_info
        .query_pools(&router.wrap(), &pair_info.contract_addr)
        .unwrap();
    assert_eq!(balances[0].amount.u128(), 100_000);
    assert_eq!(balances[1].amount.u128(), 100_000);
}
