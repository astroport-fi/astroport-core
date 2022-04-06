use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{
    ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType,
    QueryMsg as FactoryQueryMsg,
};

use astroport::router::{
    ExecuteMsg as RouterExecuteMsg, InstantiateMsg as RouterInstantiateMsg, SwapOperation,
};

use astroport::pair_anchor::ExecuteMsg;

mod mock_anchor_contract;
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use mock_anchor_contract::AnchorInstantiateMsg;

use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{to_binary, Addr, Coin, Decimal, Uint128};
use cw20::{Cw20Coin, Cw20ExecuteMsg, MinterResponse};
use moneymarket::market::ExecuteMsg as AnchorExecuteMsg;
use terra_multi_test::{AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock};

const OWNER: &str = "owner";

fn mock_app(bank: BankKeeper) -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
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
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );

    app.store_code(pair_contract)
}

fn store_pair_anchor_code(app: &mut TerraApp) -> u64 {
    let pair_anchor_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_pair_anchor::contract::execute,
        astroport_pair_anchor::contract::instantiate,
        astroport_pair_anchor::contract::query,
    ));

    app.store_code(pair_anchor_contract)
}

fn store_router_code(app: &mut TerraApp) -> u64 {
    let router_contract = Box::new(ContractWrapper::new(
        astroport_router::contract::execute,
        astroport_router::contract::instantiate,
        astroport_factory::contract::query,
    ));

    app.store_code(router_contract)
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

fn store_anchor_code(app: &mut TerraApp) -> u64 {
    let factory_contract = Box::new(ContractWrapper::new_with_empty(
        crate::mock_anchor_contract::execute,
        crate::mock_anchor_contract::instantiate,
        crate::mock_anchor_contract::query,
    ));

    app.store_code(factory_contract)
}

#[test]
fn test_compatibility_of_pair_anchor_with_routeswap() {
    let bank = BankKeeper::new();
    let mut app = mock_app(bank);

    let owner = Addr::unchecked(OWNER);
    let alice_address = Addr::unchecked("alice");

    app.init_bank_balance(
        &alice_address,
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(2000_000000u128),
            },
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(10000_000000u128),
            },
        ],
    )
    .unwrap();

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let router_code_id = store_router_code(&mut app);
    let pair_anchor_code_id = store_pair_anchor_code(&mut app);
    let pair_luna_code_id = store_pair_code(&mut app);
    let anchor_code_id = store_anchor_code(&mut app);

    let anchor_contract = app
        .instantiate_contract(
            anchor_code_id,
            owner.clone(),
            &AnchorInstantiateMsg {},
            &[],
            "ANCHOR",
            None,
        )
        .unwrap();

    let token_name = "aUST";

    let init_token_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 6,
        initial_balances: vec![Cw20Coin {
            address: alice_address.to_string(),
            amount: Uint128::from(10000_000000u128),
        }],
        mint: Some(MinterResponse {
            minter: anchor_contract.to_string(),
            cap: None,
        }),
    };

    let token_aust_contract = app
        .instantiate_contract(
            token_code_id,
            owner.clone(),
            &init_token_msg,
            &[],
            token_name,
            None,
        )
        .unwrap();

    let msg = AnchorExecuteMsg::RegisterContracts {
        overseer_contract: token_aust_contract.to_string(),
        interest_model: "".to_string(),
        distribution_model: "".to_string(),
        collector_contract: "".to_string(),
        distributor_contract: "".to_string(),
    };

    app.execute_contract(owner.clone(), anchor_contract.clone(), &msg, &[])
        .unwrap();

    let init_factory = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![
            PairConfig {
                code_id: pair_luna_code_id,
                maker_fee_bps: 0,
                pair_type: PairType::Xyk {},
                total_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: false,
            },
            PairConfig {
                code_id: pair_anchor_code_id,
                maker_fee_bps: 0,
                pair_type: PairType::Custom("Anchor-XYK".to_string()),
                total_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: true,
            },
        ],
        token_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
    };

    let factory_contract = app
        .instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_factory,
            &[],
            "FACTORY",
            None,
        )
        .unwrap();

    let init_router = RouterInstantiateMsg {
        astroport_factory: factory_contract.to_string(),
    };

    let router_contract = app
        .instantiate_contract(
            router_code_id,
            owner.clone(),
            &init_router,
            &[],
            "ROUTER",
            None,
        )
        .unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        pair_type: PairType::Xyk {},
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            AssetInfo::Token {
                contract_addr: token_aust_contract.clone(),
            },
        ],
        init_params: None,
    };

    app.execute_contract(owner.clone(), factory_contract.clone(), &msg, &[])
        .unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        pair_type: PairType::Custom("Anchor-XYK".to_string()),
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: token_aust_contract.clone(),
            },
        ],
        init_params: Some(to_binary(&anchor_contract.to_string()).unwrap()),
    };

    app.execute_contract(owner.clone(), factory_contract.clone(), &msg, &[])
        .unwrap();

    let msg = FactoryQueryMsg::Pair {
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            AssetInfo::Token {
                contract_addr: token_aust_contract.clone(),
            },
        ],
    };

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(&factory_contract, &msg)
        .unwrap();

    let pair_luna_instance = res.contract_addr;

    let aust_amount = Uint128::from(2000_000000u128);
    let luna_amount = Uint128::from(1000_000000u128);

    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: pair_luna_instance.to_string(),
        expires: None,
        amount: aust_amount,
    };

    app.execute_contract(
        alice_address.clone(),
        token_aust_contract.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: token_aust_contract.clone(),
                },
                amount: aust_amount,
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                amount: luna_amount,
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    app.execute_contract(
        alice_address.clone(),
        pair_luna_instance.clone(),
        &msg,
        &[Coin {
            denom: "uluna".to_string(),
            amount: luna_amount,
        }],
    )
    .unwrap();

    let route_swap_msg = RouterExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: token_aust_contract.clone(),
                },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: token_aust_contract.clone(),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
        ],
        minimum_receive: None,
        to: None,
        max_spread: Some(Decimal::percent(50)),
    };

    app.execute_contract(
        alice_address.clone(),
        router_contract.clone(),
        &route_swap_msg,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(1000_000000u128),
        }],
    )
    .unwrap();

    let luna_balance = app.wrap().query_balance(&alice_address, "uluna").unwrap();
    let uusd_balance = app.wrap().query_balance(&alice_address, "uusd").unwrap();
    let aust_balance: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &token_aust_contract,
            &cw20::Cw20QueryMsg::Balance {
                address: alice_address.to_string(),
            },
        )
        .unwrap();

    // 10000 LUNA, Deposit 1000 LUNA to LP, Receive 290+ Luna from Swap
    assert_eq!(luna_balance.amount.u128(), 9290_675961u128);
    assert_eq!(uusd_balance.amount.u128(), 998_610000u128);
    assert_eq!(aust_balance.balance.u128(), 8000_000000u128);

    let route_swap_msg = RouterExecuteMsg::ExecuteSwapOperations {
        operations: vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token {
                    contract_addr: token_aust_contract.clone(),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            },
            SwapOperation::NativeSwap {
                offer_denom: "uusd".to_string(),
                ask_denom: "uluna".to_string(),
            },
        ],
        minimum_receive: None,
        to: None,
        max_spread: Some(Decimal::percent(50)),
    };
    let cw20_send_msg = Cw20ExecuteMsg::Send {
        contract: router_contract.to_string(),
        amount: Uint128::from(100_000000u128),
        msg: to_binary(&route_swap_msg).unwrap(),
    };

    app.execute_contract(
        alice_address.clone(),
        token_aust_contract.clone(),
        &cw20_send_msg,
        &[],
    )
    .unwrap();

    let luna_balance = app.wrap().query_balance(&alice_address, "uluna").unwrap();
    let uusd_balance = app.wrap().query_balance(&alice_address, "uusd").unwrap();
    let aust_balance: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &token_aust_contract,
            &cw20::Cw20QueryMsg::Balance {
                address: alice_address.to_string(),
            },
        )
        .unwrap();

    // 100 aUST = 121.673652 UST ~ 1.62 LUNA
    assert_eq!(luna_balance.amount.u128(), 9292_220033u128);
    // UST balance was not changed
    assert_eq!(uusd_balance.amount.u128(), 998_610000u128);
    // Spent 2 aUST
    assert_eq!(aust_balance.balance.u128(), 7900_000000u128);
}
