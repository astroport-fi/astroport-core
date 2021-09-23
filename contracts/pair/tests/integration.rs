use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::pair::{ExecuteMsg, InstantiateMsg, QueryMsg};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, Coin, QueryRequest, Uint128, WasmQuery};
use cw_multi_test::{App, BankKeeper, ContractWrapper, Executor};

fn mock_app() -> App {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper {};

    App::new(api, env.block, bank, MockStorage::new())
}

fn instantiate_pair(router: &mut App, owner: Addr) -> Addr {
    let token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let token_contract_code_id = router.store_code(token_contract);

    let pair_contract = Box::new(ContractWrapper::new(
        astroport_pair::contract::execute,
        astroport_pair::contract::instantiate,
        astroport_pair::contract::query,
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
        pair_type: PairType::Xyk {},
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

    pair
}

#[test]
fn test_provide_and_withdraw_liquidity() {
    let owner = Addr::unchecked("owner");
    let alice_address = Addr::unchecked("alice");
    let mut router = mock_app();

    // Set alice balances
    router
        .init_bank_balance(
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
        .init_bank_balance(
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
        res.events[1].attributes[1],
        attr("action", "provide_liquidity")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("assets", "100uusd, 100uluna")
    );
    assert_eq!(
        res.events[1].attributes[3],
        attr("share", 100u128.to_string())
    );
    assert_eq!(res.events[3].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[3].attributes[2], attr("to", "alice"));
    assert_eq!(res.events[3].attributes[3], attr("amount", 100.to_string()));
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
