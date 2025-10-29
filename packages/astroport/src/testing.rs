use crate::asset::{format_lp_token_name, Asset, AssetInfo, PairInfo};
use crate::mock_querier::mock_dependencies;
use crate::querier::query_pair_info;

use crate::factory::PairType;
use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
use cosmwasm_std::{to_json_binary, Addr, BankMsg, Coin, CosmosMsg, Empty, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

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
        token_asset.into_msg(Addr::unchecked("addr0000")).unwrap(),
        CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
            contract_addr: String::from("asset0000"),
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                recipient: String::from("addr0000"),
                amount: Uint128::new(123123u128),
            })
            .unwrap(),
            funds: vec![],
        })
    );

    assert_eq!(
        native_token_asset
            .into_msg(Addr::unchecked("addr0000"))
            .unwrap(),
        CosmosMsg::<Empty>::Bank(BankMsg::Send {
            to_address: String::from("addr0000"),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(123123u128),
            }]
        })
    );
}

#[test]
fn test_format_lp_token_name() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_astroport_pairs(&[(
        &"asset0000uusd".to_string(),
        &PairInfo {
            asset_infos: vec![
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0000"),
            liquidity_token: "liquidity0000".to_owned(),
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

    let lp_name = format_lp_token_name(&pair_info.asset_infos, &deps.as_ref().querier).unwrap();
    assert_eq!(lp_name, "MAPP-UUSD-LP")
}
