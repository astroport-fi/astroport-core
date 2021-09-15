use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, Decimal, QueryRequest, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use astroport::factory::{PairConfig, PairType};
use maker::msg::{ExecuteMsg, InstantiateMsg};

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
}

fn instantiate_contracts(router: &mut App, owner: Addr, staking: Addr) -> (Addr, Addr, Addr) {
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
        maker::contract::execute,
        maker::contract::instantiate,
        maker::contract::query,
    ));
    let market_code_id = router.store_code(maker_contract);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        staking_contract: staking.to_string(),
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
    token1: &Addr,
    token2: &Addr,
    amount1: Uint128,
    amount2: Uint128,
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
    let asset_infos = [
        AssetInfo::Token {
            contract_addr: token1.clone(),
        },
        AssetInfo::Token {
            contract_addr: token2.clone(),
        },
    ];

    let msg = astroport::factory::ExecuteMsg::CreatePair {
        pair_type: PairType::Xyk {},
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

#[test]
fn collect_all() {
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

    // Mint all tokens for maker
    for t in vec![
        (&usdc_token_instance, &astro_token_instance),
        (&luna_token_instance, &astro_token_instance),
        (&usdc_token_instance, &luna_token_instance),
    ] {
        let (instance_a, instance_b) = t;
        create_pair(
            &mut router,
            owner.clone(),
            user.clone(),
            &factory_instance,
            instance_a,
            instance_b,
            Uint128::from(100_000u128),
            Uint128::from(100_000u128),
        );
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

    let msg = ExecuteMsg::Collect {
        start_after: None,
        limit: None,
    };

    router
        .execute_contract(maker_instance.clone(), maker_instance.clone(), &msg, &[])
        .unwrap();

    for t in vec![
        (astro_token_instance.clone(), 60u128), // 10 astro + 20 usdc + 30 luna
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

        // Check staking balance
        check_balance(&mut router, staking.clone(), token, Uint128::new(amount));
    }
}
