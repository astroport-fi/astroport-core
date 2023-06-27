use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{
    ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg, PairConfig, PairType,
    QueryMsg as FactoryQueryMsg,
};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, TWAP_PRECISION,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cosmwasm_std::{attr, to_binary, Addr, Coin, Decimal, QueryRequest, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use classic_test_tube::{TerraTestApp, Wasm, SigningAccount, Module, Account, Bank};

fn store_token_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let astro_token_contract = std::fs::read("../../../artifacts/astroport_token.wasm").unwrap();
    let contract = wasm.store_code(&astro_token_contract, None, owner).unwrap();
    contract.data.code_id
}

fn store_pair_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let pair_contract = std::fs::read("../../../artifacts/astroport_pair_stable.wasm").unwrap();
    let contract = wasm.store_code(&pair_contract, None, owner).unwrap();
    contract.data.code_id
}

fn store_factory_code(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount) -> u64 {
    let factory_contract = std::fs::read("../../../artifacts/astroport_factory.wasm").unwrap();
    let contract = wasm.store_code(&factory_contract, None, owner).unwrap();
    contract.data.code_id
}

fn instantiate_pair<'a>(app: &'a TerraTestApp, owner: &'a SigningAccount) -> String {
    let wasm = Wasm::new(app);

    let token_contract_code_id = store_token_code(&wasm, owner);

    let pair_contract_code_id = store_pair_code(&wasm, owner);

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

    let pair = wasm
        .instantiate(
        pair_contract_code_id,
        &msg,
        Some(&owner.address()),
        Some("PAIR"),
        &[],
        owner
        )   
        .unwrap();

    let res: PairInfo = wasm
        .query(&pair.data.address, &QueryMsg::Pair {})
        .unwrap();
    assert_eq!("contract #0", res.contract_addr);
    assert_eq!("contract #1", res.liquidity_token);

    pair.data.address
}

#[test]
fn test_provide_and_withdraw_liquidity() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        2
    ).unwrap();
    let owner = &accs[0];
    let alice_address = &accs[1];

    // Init pair
    let pair_instance = instantiate_pair(&app, owner);

    let res: Result<PairInfo, _> = wasm.query(&pair_instance.to_string(), &QueryMsg::Pair {});
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
    // router
    //     .init_bank_balance(
    //         &pair_instance,
    //         vec![
    //             Coin {
    //                 denom: "uusd".to_string(),
    //                 amount: Uint128::new(100u128),
    //             },
    //             Coin {
    //                 denom: "uluna".to_string(),
    //                 amount: Uint128::new(100u128),
    //             },
    //         ],
    //     )
    //     .unwrap();

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(Uint128::new(100), Uint128::new(100), None, None);
    let res = wasm.execute(&pair_instance, &msg, &coins, alice_address).unwrap();

    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "provide_liquidity")
    );
    assert_eq!(res.events[1].attributes[3], attr("receiver", "alice"),);
    assert_eq!(
        res.events[1].attributes[4],
        attr("assets", "100uusd, 100uluna")
    );
    assert_eq!(
        res.events[1].attributes[5],
        attr("share", 100u128.to_string())
    );
    assert_eq!(res.events[3].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[3].attributes[2], attr("to", "alice"));
    assert_eq!(res.events[3].attributes[3], attr("amount", 100.to_string()));

    // Provide liquidity for receiver
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(100),
        Uint128::new(100),
        Some("bob".to_string()),
        None,
    );
    let res = wasm.execute(&pair_instance, &msg, &coins, alice_address).unwrap();

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
        attr("share", 50u128.to_string())
    );
    assert_eq!(res.events[3].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[3].attributes[2], attr("to", "bob"));
    assert_eq!(res.events[3].attributes[3], attr("amount", 50.to_string()));
}

fn provide_liquidity_msg(
    uusd_amount: Uint128,
    uluna_amount: Uint128,
    receiver: Option<String>,
    slippage_tolerance: Option<Decimal>,
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
        slippage_tolerance: Option::from(slippage_tolerance),
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
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let owner = &app.init_account(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
    ).unwrap();

    let token_code_id = store_token_code(&wasm, owner);

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
            address: owner.address().to_string(),
            amount: x_amount + x_offer,
        }],
        mint: Some(MinterResponse {
            minter: owner.address().to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let token_x_instance = wasm
        .instantiate(
            token_code_id,
            &init_msg,
            Some(&owner.address()),
            Some(token_name),
            &[],
            owner,
        )
        .unwrap();

    let token_name = "Ytoken";

    let init_msg = TokenInstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: 7,
        initial_balances: vec![Cw20Coin {
            address: owner.address().to_string(),
            amount: y_amount,
        }],
        mint: Some(MinterResponse {
            minter: owner.address().to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let token_y_instance = wasm
        .instantiate(
            token_code_id,
            &init_msg,
            Some(&owner.address()),
            Some(token_name),
            &[],
            owner,
        )
        .unwrap();

    let pair_code_id = store_pair_code(&wasm, owner);
    let factory_code_id = store_factory_code(&wasm, owner);

    let init_msg = FactoryInstantiateMsg {
        fee_address: None,
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            maker_fee_bps: 0,
            pair_type: PairType::Xyk {},
            total_fee_bps: 0,
            is_disabled: None,
        }],
        token_code_id,
        generator_address: Some(String::from("generator")),
        owner: owner.address().to_string(),
        whitelist_code_id: 234u64,
    };

    let factory_instance = wasm
        .instantiate(
            factory_code_id, 
            &init_msg, 
            Some(owner.address().as_str()), 
            Some("FACTORY"), 
            &[], 
            owner
        ).unwrap();

    let msg = FactoryExecuteMsg::CreatePair {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: Addr::unchecked(&token_x_instance.data.address),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked(&token_y_instance.data.address),
            },
        ],
        pair_type: PairType::Xyk {},
        init_params: None,
    };

    wasm.execute(&factory_instance.data.address, &msg,&[], owner).unwrap();

    let msg = FactoryQueryMsg::Pair {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: Addr::unchecked(&token_x_instance.data.address),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked(&token_y_instance.data.address),
            },
        ],
    };

    let res: PairInfo = wasm
        .query(&factory_instance.data.address, &msg)
        .unwrap();

    let pair_instance = res.contract_addr;

    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: pair_instance.to_string(),
        expires: None,
        amount: x_amount + x_offer,
    };

    wasm.execute(&token_x_instance.data.address, &msg, &[], owner).unwrap();

    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: pair_instance.to_string(),
        expires: None,
        amount: y_amount,
    };

    wasm.execute(&token_y_instance.data.address, &msg, &[], owner).unwrap();

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked(&token_x_instance.data.address),
                },
                amount: x_amount,
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked(&token_y_instance.data.address),
                },
                amount: y_amount,
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    wasm.execute(pair_instance.as_str(), &msg, &[], owner).unwrap();

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

    wasm.execute(&token_x_instance.data.address, &msg, &[], owner).unwrap();

    let msg = Cw20QueryMsg::Balance {
        address: user.to_string(),
    };

    let res: BalanceResponse = wasm
        .query(&token_y_instance.data.address, &msg)
        .unwrap();

    let acceptable_spread_amount = Uint128::new(10);

    assert_eq!(res.balance, y_expected_return - acceptable_spread_amount);
}

#[test]
fn test_if_twap_is_calculated_correctly_when_pool_idles() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let user1 = &app.init_account(
        &[
            Coin::new(4000000_000000, "uusd"),
            Coin::new(2000000_000000, "uluna"),
        ],
    ).unwrap();

    // instantiate pair
    let pair_instance = instantiate_pair(&app, user1);

    // provide liquidity, accumulators are empty
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(1000000_000000),
        Uint128::new(1000000_000000),
        None,
        Option::from(Decimal::one()),
    );
    wasm.execute(&pair_instance, &msg, &coins, user1).unwrap();

    const BLOCKS_PER_DAY: u64 = 17280;
    const ELAPSED_SECONDS: u64 = BLOCKS_PER_DAY * 5;

    // a day later
    app.increase_time(ELAPSED_SECONDS);

    // provide liquidity, accumulators firstly filled with the same prices
    let (msg, coins) = provide_liquidity_msg(
        Uint128::new(2000000_000000),
        Uint128::new(1000000_000000),
        None,
        Some(Decimal::percent(50)),
    );
    wasm.execute(&pair_instance, &msg, &coins, user1).unwrap();

    // get current twap accumulator values
    let msg = QueryMsg::CumulativePrices {};
    let cpr_old: CumulativePricesResponse = wasm.query(&pair_instance, &msg).unwrap();

    // a day later
    app.increase_time(ELAPSED_SECONDS);

    // get current twap accumulator values, it should be added up by the query method with new 2/1 ratio
    let msg = QueryMsg::CumulativePrices {};
    let cpr_new: CumulativePricesResponse = wasm.query(&pair_instance, &msg).unwrap();

    let twap0 = cpr_new.price0_cumulative_last - cpr_old.price0_cumulative_last;
    let twap1 = cpr_new.price1_cumulative_last - cpr_old.price1_cumulative_last;

    // Prices weren't changed for the last day, uusd amount in pool = 3000000_000000, uluna = 2000000_000000
    // In accumulators we don't have any precision so we rely on elapsed time to not consider it
    let price_precision = Uint128::from(10u128.pow(TWAP_PRECISION.into()));
    assert_eq!(twap0 / price_precision, Uint128::new(57600)); // 0.666666 * ELAPSED_SECONDS (86400)
    assert_eq!(twap1 / price_precision, Uint128::new(129600)); //   1.5 * ELAPSED_SECONDS
}

#[test]
fn create_pair_with_same_assets() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    let owner = &app.init_account(
        &[
            Coin::new(233u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
    ).unwrap();

    let token_contract_code_id = store_token_code(&wasm, owner);
    let pair_contract_code_id = store_pair_code(&wasm, owner);

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

    let resp = wasm.instantiate(
        pair_contract_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some("PAIR"), 
        &[], 
        owner
    ).unwrap_err();

    assert_eq!(resp.to_string(), "Doubling assets in asset infos")
}
