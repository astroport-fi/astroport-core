use cosmwasm_std::{
    attr, coin, to_binary, Addr, BalanceResponse as NativeBalanceResponse, BankQuery, Coin,
    QueryRequest, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse, TokenInfoResponse};

use astroport::asset::{native_asset_info, token_asset_info, AssetInfo};
use astroport::native_coin_wrapper::{Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport::token::InstantiateMsg as AstroInstantiateMsg;
use cw_multi_test::{App, ContractWrapper, Executor};

fn mock_app(owner: Addr, coins: Vec<Coin>) -> App {
    App::new(|router, _, storage| {
        router.bank.init_balance(storage, &owner, coins).unwrap();
    })
}

fn check_balance(app: &mut App, user: Addr, asset_info: &AssetInfo) -> Uint128 {
    match asset_info {
        AssetInfo::Token { contract_addr } => {
            let res: Result<BalanceResponse, _> =
                app.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&Cw20QueryMsg::Balance {
                        address: user.to_string(),
                    })
                    .unwrap(),
                }));

            res.unwrap().balance
        }
        AssetInfo::NativeToken { denom } => {
            let res: Result<NativeBalanceResponse, _> =
                app.wrap().query(&QueryRequest::Bank(BankQuery::Balance {
                    address: user.to_string(),
                    denom: denom.to_string(),
                }));

            res.unwrap().amount.amount
        }
    }
}

fn store_astro_code_id(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn create_astro_token(app: &mut App, astro_token_code_id: u64, owner: &Addr) -> Addr {
    let msg = AstroInstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: Some(Uint128::new(100000000000)),
        }),
        marketing: None,
    };

    app.instantiate_contract(
        astro_token_code_id,
        owner.clone(),
        &msg,
        &[],
        String::from("ASTRO"),
        None,
    )
    .unwrap()
}

fn mint_some_astro(
    router: &mut App,
    owner: Addr,
    astro_token_instance: Addr,
    to: &str,
    amount: Uint128,
) {
    let res = router
        .execute_contract(
            owner.clone(),
            astro_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Mint {
                recipient: String::from(to),
                amount,
            },
            &[],
        )
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

fn store_native_wrapper_code(app: &mut App) -> u64 {
    let contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_native_coin_wrapper::contract::execute,
            astroport_native_coin_wrapper::contract::instantiate,
            astroport_native_coin_wrapper::contract::query,
        )
        .with_reply_empty(astroport_native_coin_wrapper::contract::reply),
    );

    app.store_code(contract)
}

#[test]
fn proper_initialization() {
    let owner = Addr::unchecked("owner");
    let mut app = mock_app(owner.clone(), vec![]);

    let native_wrapper_code_id = store_native_wrapper_code(&mut app);
    let astro_token_code_id = store_astro_code_id(&mut app);

    let native_wrapper_instance = app
        .instantiate_contract(
            native_wrapper_code_id,
            Addr::unchecked(owner.clone()),
            &InstantiateMsg {
                denom: "ibc/EBD5A24C554198EBAF44979C5B4D2C2D312E6EBAB71962C92F735499C7575839"
                    .to_string(),
                token_code_id: astro_token_code_id,
                token_decimals: 15,
            },
            &[],
            "CW20 native tokens wrapper contract",
            None,
        )
        .unwrap();

    let config_res: Config = app
        .wrap()
        .query_wasm_smart(&native_wrapper_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(
        "ibc/EBD5A24C554198EBAF44979C5B4D2C2D312E6EBAB71962C92F735499C7575839".to_string(),
        config_res.denom.to_string()
    );
    assert_eq!("contract1", config_res.token.to_string());

    let token_res: TokenInfoResponse = app
        .wrap()
        .query_wasm_smart(&config_res.token, &Cw20QueryMsg::TokenInfo {})
        .unwrap();
    assert_eq!("IBC/EBD5", token_res.symbol.to_string());
    assert_eq!(
        "CW20-wrapped ibc/EBD5A24C554198EBAF44979C5B4D2C2D3",
        token_res.name.to_string()
    );
    assert_eq!("15", token_res.decimals.to_string());
}

#[test]
fn check_wrap_and_unwrap() {
    let owner = Addr::unchecked("owner");
    let user1 = Addr::unchecked("user1");
    let mut app = mock_app(
        owner.clone(),
        vec![
            Coin {
                denom: "ibc/EBD5A24C554198EBA".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
            Coin {
                denom: "wrapped_coin_1".to_string(),
                amount: Uint128::new(100_000_000_000u128),
            },
        ],
    );

    // Send native asset to user1
    app.send_tokens(
        owner.clone(),
        user1.clone(),
        &[coin(100, "ibc/EBD5A24C554198EBA".to_string())],
    )
    .unwrap();

    let native_wrapper_code_id = store_native_wrapper_code(&mut app);
    let astro_token_code_id = store_astro_code_id(&mut app);
    let astro_token_addr = create_astro_token(&mut app, astro_token_code_id, &owner);

    let native_wrapper_instance = app
        .instantiate_contract(
            native_wrapper_code_id,
            Addr::unchecked(owner.clone()),
            &InstantiateMsg {
                denom: "ibc/EBD5A24C554198EBA".to_string(),
                token_code_id: astro_token_code_id,
                token_decimals: 6,
            },
            &[],
            "CW20 native tokens wrapper contract",
            None,
        )
        .unwrap();

    let res = app
        .wrap()
        .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: native_wrapper_instance.to_string(),
            msg: to_binary(&QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();
    let wrapped_cw20_native_token = token_asset_info(res.token);
    assert_eq!("contract2", wrapped_cw20_native_token.to_string());

    let err = app
        .execute_contract(
            Addr::unchecked("user1"),
            native_wrapper_instance.clone(),
            &ExecuteMsg::Wrap {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "No funds sent");

    let err = app
        .execute_contract(
            Addr::unchecked("owner"),
            native_wrapper_instance.clone(),
            &ExecuteMsg::Wrap {},
            &[
                coin(20, "ibc/EBD5A24C554198EBA"),
                coin(30, "wrapped_coin_1"),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Sent more than one denomination"
    );

    // try to unwrap cw20 tokens to get native tokens
    let err = app
        .execute_contract(
            user1.clone(),
            Addr::unchecked(wrapped_cw20_native_token.to_string()),
            &Cw20ExecuteMsg::Send {
                contract: native_wrapper_instance.to_string(),
                msg: to_binary(&Cw20HookMsg::Unwrap {}).unwrap(),
                amount: Uint128::from(10u128),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Cannot Sub with 0 and 10", err.root_cause().to_string());

    // check user1's wrapped cw20 token balance
    assert_eq!(
        check_balance(&mut app, user1.clone(), &wrapped_cw20_native_token),
        Uint128::new(0)
    );

    app.execute_contract(
        Addr::unchecked("user1"),
        native_wrapper_instance.clone(),
        &ExecuteMsg::Wrap {},
        &[coin(20, "ibc/EBD5A24C554198EBA")],
    )
    .unwrap();

    // check user1's native coin balance
    assert_eq!(
        check_balance(
            &mut app,
            user1.clone(),
            &native_asset_info("ibc/EBD5A24C554198EBA".to_string())
        ),
        Uint128::new(80)
    );

    // check user1's wrapped cw20 token balance
    assert_eq!(
        check_balance(&mut app, user1.clone(), &wrapped_cw20_native_token),
        Uint128::new(20)
    );

    // check wrapper's wrapped cw20 token balance
    assert_eq!(
        check_balance(
            &mut app,
            native_wrapper_instance.clone(),
            &wrapped_cw20_native_token
        ),
        Uint128::new(0)
    );

    // check wrapper's native coin balance
    assert_eq!(
        check_balance(
            &mut app,
            native_wrapper_instance.clone(),
            &native_asset_info("ibc/EBD5A24C554198EBA".to_string())
        ),
        Uint128::new(20)
    );

    mint_some_astro(
        &mut app,
        owner.clone(),
        astro_token_addr.clone(),
        owner.as_str(),
        Uint128::new(100),
    );

    // try to unwrap cw20 tokens from the other cw20 token.
    let resp = app
        .execute_contract(
            owner.clone(),
            astro_token_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: native_wrapper_instance.to_string(),
                msg: to_binary(&Cw20HookMsg::Unwrap {}).unwrap(),
                amount: Uint128::from(10u128),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(resp.root_cause().to_string(), "Unauthorized");

    // try to unwrap cw20 tokens from our cw20 token.
    app.execute_contract(
        user1.clone(),
        Addr::unchecked(wrapped_cw20_native_token.to_string()),
        &Cw20ExecuteMsg::Send {
            contract: native_wrapper_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::Unwrap {}).unwrap(),
            amount: Uint128::from(10u128),
        },
        &[],
    )
    .unwrap();

    // check user1's balances
    assert_eq!(
        check_balance(&mut app, user1.clone(), &wrapped_cw20_native_token),
        Uint128::new(10)
    );
    assert_eq!(
        check_balance(
            &mut app,
            user1.clone(),
            &native_asset_info("ibc/EBD5A24C554198EBA".to_string())
        ),
        Uint128::new(90)
    );

    // check wrapper's balances
    assert_eq!(
        check_balance(
            &mut app,
            native_wrapper_instance.clone(),
            &wrapped_cw20_native_token
        ),
        Uint128::zero()
    );
    assert_eq!(
        check_balance(
            &mut app,
            native_wrapper_instance.clone(),
            &native_asset_info("ibc/EBD5A24C554198EBA".to_string())
        ),
        Uint128::new(10)
    );
}
