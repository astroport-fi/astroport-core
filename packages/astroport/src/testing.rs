use crate::asset::{format_lp_token_name, Asset, AssetInfo, PairInfo};
use crate::mock_querier::mock_dependencies;
use crate::querier::{
    query_all_balances, query_balance, query_pair_info, query_supply, query_token_balance,
};

use crate::factory::PairType;
use crate::DecimalCheckedOps;
use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
use cosmwasm_std::{to_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

#[test]
fn token_balance_querier() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_token_balances(&[(
        &String::from("liquidity0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128))],
    )]);

    deps.querier.with_cw20_query_handler();
    assert_eq!(
        Uint128::new(123u128),
        query_token_balance(
            &deps.as_ref().querier,
            Addr::unchecked("liquidity0000"),
            Addr::unchecked(MOCK_CONTRACT_ADDR),
        )
        .unwrap()
    );
    deps.querier.with_default_query_handler()
}

#[test]
fn balance_querier() {
    let deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(200u128),
    }]);

    assert_eq!(
        query_balance(
            &deps.as_ref().querier,
            Addr::unchecked(MOCK_CONTRACT_ADDR),
            "uusd".to_string()
        )
        .unwrap(),
        Uint128::new(200u128)
    );
}

#[test]
fn all_balances_querier() {
    let deps = mock_dependencies(&[
        Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(200u128),
        },
        Coin {
            denom: "ukrw".to_string(),
            amount: Uint128::new(300u128),
        },
    ]);

    assert_eq!(
        query_all_balances(&deps.as_ref().querier, Addr::unchecked(MOCK_CONTRACT_ADDR),).unwrap(),
        vec![
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(200u128),
            },
            Coin {
                denom: "ukrw".to_string(),
                amount: Uint128::new(300u128),
            }
        ]
    );
}

#[test]
fn supply_querier() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_token_balances(&[(
        &String::from("liquidity0000"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128)),
            (&String::from("addr00000"), &Uint128::new(123u128)),
            (&String::from("addr00001"), &Uint128::new(123u128)),
            (&String::from("addr00002"), &Uint128::new(123u128)),
        ],
    )]);

    deps.querier.with_cw20_query_handler();

    assert_eq!(
        query_supply(&deps.as_ref().querier, Addr::unchecked("liquidity0000")).unwrap(),
        Uint128::new(492u128)
    )
}

#[test]
fn test_asset_info() {
    let token_info: AssetInfo = AssetInfo::Token {
        contract_addr: Addr::unchecked("asset0000"),
    };
    let native_token_info: AssetInfo = AssetInfo::NativeToken {
        denom: "uusd".to_string(),
    };

    assert_eq!(false, token_info.equal(&native_token_info));

    assert_eq!(
        false,
        token_info.equal(&AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0001"),
        })
    );

    assert_eq!(
        true,
        token_info.equal(&AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        })
    );

    assert_eq!(true, native_token_info.is_native_token());
    assert_eq!(false, token_info.is_native_token());

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(123),
    }]);
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128)),
            (&String::from("addr00000"), &Uint128::new(123u128)),
            (&String::from("addr00001"), &Uint128::new(123u128)),
            (&String::from("addr00002"), &Uint128::new(123u128)),
        ],
    )]);

    assert_eq!(
        native_token_info
            .query_pool(&deps.as_ref().querier, Addr::unchecked(MOCK_CONTRACT_ADDR))
            .unwrap(),
        Uint128::new(123u128)
    );
    deps.querier.with_cw20_query_handler();
    assert_eq!(
        token_info
            .query_pool(&deps.as_ref().querier, Addr::unchecked(MOCK_CONTRACT_ADDR))
            .unwrap(),
        Uint128::new(123u128)
    );
}

#[test]
fn test_asset() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(123),
    }]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128)),
            (&String::from("addr00000"), &Uint128::new(123u128)),
            (&String::from("addr00001"), &Uint128::new(123u128)),
            (&String::from("addr00002"), &Uint128::new(123u128)),
        ],
    )]);

    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::new(1000000u128))],
    );

    let token_asset = Asset {
        amount: Uint128::new(123123u128),
        info: AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
    };

    let native_token_asset = Asset {
        amount: Uint128::new(123123u128),
        info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
    };

    assert_eq!(
        token_asset.compute_tax(&deps.as_ref().querier).unwrap(),
        Uint128::zero()
    );
    assert_eq!(
        native_token_asset
            .compute_tax(&deps.as_ref().querier)
            .unwrap(),
        Uint128::new(1220u128)
    );

    assert_eq!(
        native_token_asset
            .deduct_tax(&deps.as_ref().querier)
            .unwrap(),
        Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(121903u128),
        }
    );

    assert_eq!(
        token_asset
            .into_msg(&deps.as_ref().querier, Addr::unchecked("addr0000"))
            .unwrap(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("asset0000"),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: String::from("addr0000"),
                amount: Uint128::new(123123u128),
            })
            .unwrap(),
            funds: vec![],
        })
    );

    assert_eq!(
        native_token_asset
            .into_msg(&deps.as_ref().querier, Addr::unchecked("addr0000"))
            .unwrap(),
        CosmosMsg::Bank(BankMsg::Send {
            to_address: String::from("addr0000"),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(121903u128),
            }]
        })
    );
}

#[test]
fn query_astroport_pair_contract() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_astroport_pairs(&[(
        &"asset0000uusd".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0000"),
            liquidity_token: Addr::unchecked("liquidity0000"),
            pair_type: PairType::Xyk {},
        },
    )]);

    let pair_info: PairInfo = query_pair_info(
        &deps.as_ref().querier,
        Addr::unchecked(MOCK_CONTRACT_ADDR),
        &[
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    )
    .unwrap();

    assert_eq!(pair_info.contract_addr, String::from("pair0000"),);
    assert_eq!(pair_info.liquidity_token, String::from("liquidity0000"),);
}

#[test]
fn test_format_lp_token_name() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_astroport_pairs(&[(
        &"asset0000uusd".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0000"),
            liquidity_token: Addr::unchecked("liquidity0000"),
            pair_type: PairType::Xyk {},
        },
    )]);

    let pair_info: PairInfo = query_pair_info(
        &deps.as_ref().querier,
        Addr::unchecked(MOCK_CONTRACT_ADDR),
        &[
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ],
    )
    .unwrap();

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128))],
    )]);

    deps.querier.with_cw20_query_handler();

    let lp_name = format_lp_token_name(pair_info.asset_infos, &deps.as_ref().querier).unwrap();
    assert_eq!(lp_name, "MAPP-UUSD-LP")
}

#[test]
fn test_decimal_checked_ops() {
    for i in 0u32..100u32 {
        let dec = Decimal::from_ratio(i, 1u32);
        assert_eq!(dec + dec, dec.checked_add(dec).unwrap());
    }
    assert!(
        Decimal::from_ratio(Uint128::MAX, Uint128::from(10u128.pow(18u32)))
            .checked_add(Decimal::one())
            .is_err()
    );

    for i in 0u128..100u128 {
        let dec = Decimal::from_ratio(i, 1u128);
        assert_eq!(
            dec * Uint128::new(i),
            dec.checked_mul(Uint128::new(i)).unwrap()
        );
    }
    assert!(
        Decimal::from_ratio(Uint128::MAX, Uint128::from(10u128.pow(18u32)))
            .checked_mul(Uint128::from(10u128.pow(18u32) + 1))
            .is_err()
    );
}
