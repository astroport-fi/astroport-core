use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Coin, Event, QueryRequest, Uint128, WasmQuery,
};
use cw_multi_test::{App, ContractWrapper, SimpleBank};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{ExecuteMsg, InstantiateMsg, QueryMsg};

fn mock_app() -> App {
    let env = mock_env();
    let api = Box::new(MockApi::default());
    let bank = SimpleBank {};

    App::new(api, env.block, bank, || Box::new(MockStorage::new()))
}

fn instantiate_pair(router: &mut App, owner: Addr) -> Addr {
    let token_contract = Box::new(ContractWrapper::new(
        terraswap_token::contract::execute,
        terraswap_token::contract::instantiate,
        terraswap_token::contract::query,
    ));

    let token_contract_code_id = router.store_code(token_contract);

    let pair_contract = Box::new(ContractWrapper::new(
        terraswap_pair::contract::execute,
        terraswap_pair::contract::instantiate,
        terraswap_pair::contract::query,
    ));

    let pair_contract_code_id = router.store_code(pair_contract);

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
        init_hook: None,
        factory_addr: Addr::unchecked("factory"),
    };

    let pair = router
        .instantiate_contract(
            pair_contract_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("PAIR"),
        )
        .unwrap();

    pair
}

#[test]
fn test_provide_and_withdraw_liquidity() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = mock_app();

    // Set alice balances
    router
        .set_bank_balance(
            &alice_address,
            vec![
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(200u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(200u128),
                },
            ],
        )
        .unwrap();

    // Init pair
    let pair_instance = instantiate_pair(&mut router, owner.clone());

    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pair_instance.to_string(),
            msg: to_binary(&QueryMsg::Pair {}).unwrap(),
        }))
        .unwrap();
    let res: PairInfo = from_binary(&res).unwrap();

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
        .set_bank_balance(
            &pair_instance,
            vec![
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(100u128),
                },
            ],
        )
        .unwrap();

    // Provide liquidity
    let (msg, coins) = provide_liquidity_msg(Uint128::new(100), Uint128::new(100));
    let res = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    assert_eq!(
        res.events,
        vec![
            Event {
                ty: String::from("wasm"),
                attributes: vec![
                    attr("contract_address", "Contract #0"),
                    attr("action", "provide_liquidity"),
                    attr("assets", "100uusd, 100uluna"),
                    attr("share", 100),
                ],
            },
            Event {
                ty: String::from("wasm"),
                attributes: vec![
                    attr("contract_address", "Contract #1"),
                    attr("action", "mint"),
                    attr("to", "alice"),
                    attr("amount", 100),
                ],
            }
        ]
    );

    // Workaround to fix balances
    router
        .set_bank_balance(
            &pair_instance,
            vec![
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(130u128),
                },
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::new(120u128),
                },
            ],
        )
        .unwrap();

    // Check kLast
    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pair_instance.to_string(),
            msg: to_binary(&QueryMsg::KLast {}).unwrap(),
        }))
        .unwrap();
    let res: Uint128 = from_binary(&res).unwrap();

    assert_eq!(res, Uint128::new(10000));

    // Provide more liquidity
    let (msg, coins) = provide_liquidity_msg(Uint128::new(30), Uint128::new(20));
    let res = router
        .execute_contract(alice_address.clone(), pair_instance.clone(), &msg, &coins)
        .unwrap();

    assert_eq!(
        res.events,
        vec![
            Event {
                ty: String::from("wasm"),
                attributes: vec![
                    attr("contract_address", "Contract #0"),
                    attr("action", "provide_liquidity"),
                    attr("assets", "30uusd, 20uluna"),
                    attr("share", 20),
                ],
            },
            Event {
                ty: String::from("wasm"),
                attributes: vec![
                    attr("contract_address", "Contract #1"),
                    attr("action", "mint"),
                    attr("to", "alice"),
                    attr("amount", 20),
                ],
            }
        ]
    );

    // kLast should increase
    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pair_instance.to_string(),
            msg: to_binary(&QueryMsg::KLast {}).unwrap(),
        }))
        .unwrap();
    let res: Uint128 = from_binary(&res).unwrap();
    assert_eq!(res, Uint128::new(15600)); // 130 * 120
}

fn provide_liquidity_msg(uusd_amount: Uint128, uluna_amount: Uint128) -> (ExecuteMsg, [Coin; 2]) {
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
    };

    let coins = [
        Coin {
            denom: "uusd".to_string(),
            amount: uusd_amount.clone(),
        },
        Coin {
            denom: "uluna".to_string(),
            amount: uluna_amount.clone(),
        },
    ];

    (msg, coins)
}
