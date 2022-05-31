use crate::contract::{
    assert_max_spread, execute, instantiate, query_pair_info, query_pool, query_reverse_simulation,
    query_share, query_simulation,
};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::InstantiateMsg;
use astroport::pair_anchor::{
    Cw20HookMsg, ExecuteMsg, PoolResponse, ReverseSimulationResponse, SimulationResponse,
};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, BankMsg, Binary, BlockInfo, Coin, CosmosMsg, Decimal, Env,
    ReplyOn, SubMsg, Timestamp, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use moneymarket::market::{Cw20HookMsg as AnchorCw20HookMsg, ExecuteMsg as AnchorExecuteMsg};

const MOCK_ANCHOR_ADDR: &str = "anchor";
const MOCK_ANCHOR_TOKEN: &str = "addr1aust";

fn create_init_params() -> Option<Binary> {
    return Some(to_binary(&MOCK_ANCHOR_ADDR.to_string()).unwrap());
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128))],
    )]);

    let msg = get_instantiate_message();

    let sender = "addr0000";
    // We can just call .unwrap() to assert this was a success
    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // It worked, let's query the state
    let pair_info: PairInfo = query_pair_info(deps.as_ref()).unwrap();
    assert_eq!(Addr::unchecked(""), pair_info.liquidity_token);
    assert_eq!(
        pair_info.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN)
            }
        ]
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(200_000000000000000000u128),
    }]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(0))],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // We can not update config for a virtual pool
    let res = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::UpdateConfig {
            params: Default::default(),
        },
    );

    assert_eq!(res, Err(ContractError::NonSupported {}))
}

#[test]
fn provide_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(200_000000000000000000u128),
    }]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(0))],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // We can not provide liquidity for a virtual pool
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                amount: Uint128::from(100_000000000000000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100_000000000000000000u128),
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000000000000000000u128),
        }],
    );
    let res = execute(deps.as_mut(), env.clone().clone(), info, msg);

    assert_eq!(res, Err(ContractError::NonSupported {}))
}

#[test]
fn withdraw_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(100u128),
    }]);

    deps.querier.with_tax(
        Decimal::zero(),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(100u128))],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Withdraw liquidity
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
        amount: Uint128::new(100u128),
    });

    let env = mock_env();
    let info = mock_info("liquidity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg);

    assert_eq!(res, Err(ContractError::NonSupported {}))
}

#[test]
fn try_native_to_token() {
    let asset_pool_amount = Uint128::new(0u128);
    let collateral_pool_amount = Uint128::new(0u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount + offer_amount, /* user deposit must be pre-applied */
    }]);

    deps.querier.with_tax(
        Decimal::zero(),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &asset_pool_amount)],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let info_contract = mock_info(env.contract.address.clone().as_str(), &[]);
    // we can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Normal swap
    let execute_msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: offer_amount,
        },
        belief_price: None,
        max_spread: Some(Decimal::percent(50)),
        to: None,
    };
    let env = mock_env_with_block_time(1000);
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: offer_amount,
        }],
    );

    let res = execute(deps.as_mut(), env.clone(), info.clone(), execute_msg).unwrap();

    // Current price is 1.216736524026807943
    // so ret_amount = 1_500_000_000 / 1.216736524026807943
    let expected_ret_amount = Uint128::new(1_232_805_928u128);

    // no spread
    let expected_spread_amount = Uint128::new(0);

    let expected_commission_amount = Uint128::new(0);
    let expected_maker_fee_amount = Uint128::new(0);

    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();
    let expected_tax_amount = Uint128::zero(); // no tax for token

    // Check simulation result
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: collateral_pool_amount, /* user deposit must be pre-applied */
        }],
    )]);

    let simulation_res: SimulationResponse = query_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: offer_amount,
        },
    )
    .unwrap();
    assert_eq!(expected_return_amount, simulation_res.return_amount);
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert_eq!(expected_spread_amount, simulation_res.spread_amount);

    // Check reverse simulation result
    let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
            amount: expected_return_amount,
        },
    )
    .unwrap();

    assert_eq!(
        (offer_amount.u128() as i128 - reverse_simulation_res.offer_amount.u128() as i128).abs()
            < 5i128,
        true
    );
    assert_eq!(
        (expected_commission_amount.u128() as i128
            - reverse_simulation_res.commission_amount.u128() as i128)
            .abs()
            < 5i128,
        true
    );
    assert_eq!(
        (expected_spread_amount.u128() as i128
            - reverse_simulation_res.spread_amount.u128() as i128)
            .abs()
            < 5i128,
        true
    );

    let first_msg = res.messages.get(0).unwrap();

    match first_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_ANCHOR_ADDR);
            assert_eq!(funds, info.funds);
            match from_binary(&msg).unwrap() {
                AnchorExecuteMsg::DepositStable {} => {}
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let second_msg = res.messages.get(1).unwrap();
    let sub_msg: ExecuteMsg;

    match second_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_CONTRACT_ADDR);
            assert_eq!(funds, vec![]);
            sub_msg = from_binary(&msg).unwrap();

            assert_eq!(
                sub_msg,
                ExecuteMsg::AssertAndSend {
                    receiver: info.sender.clone(),
                    sender: info.sender,
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: offer_amount,
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN)
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                }
            );
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    assert_eq!(res.attributes, vec![attr("action", "orchestrate"),]);

    // apply anchor deposit
    deps.querier.with_token_balances(&[(
        &String::from(MOCK_ANCHOR_TOKEN),
        &[(&String::from(MOCK_CONTRACT_ADDR), &expected_return_amount)],
    )]);

    let second_response = execute(deps.as_mut(), env.clone(), info_contract, sub_msg).unwrap();
    let msg_transfer = second_response.messages.get(0).expect("no message");

    assert_eq!(
        second_response.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "uusd"),
            attr("ask_asset", MOCK_ANCHOR_TOKEN),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
            attr("tax_amount", expected_tax_amount.to_string()),
            attr("spread_amount", expected_spread_amount.to_string()),
            attr("commission_amount", expected_commission_amount.to_string()),
            attr("maker_fee_amount", expected_maker_fee_amount.to_string()),
        ]
    );

    assert_eq!(
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from(MOCK_ANCHOR_TOKEN),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(expected_return_amount),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
        msg_transfer,
    );
}

#[test]
fn try_token_to_native() {
    let asset_pool_amount = Uint128::new(0u128);
    let collateral_pool_amount = Uint128::new(0u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount,
    }]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(
            &String::from(MOCK_CONTRACT_ADDR),
            &(asset_pool_amount + offer_amount),
        )],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Unauthorized access; can not execute swap directy for token swap
    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
            amount: offer_amount,
        },
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let sender = "addr0000";
    let env = mock_env_with_block_time(1000);
    let info = mock_info(sender, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Normal sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from(sender),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            belief_price: None,
            max_spread: Some(Decimal::percent(50)),
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info(MOCK_ANCHOR_TOKEN, &[]);
    let info_contract = mock_info(env.contract.address.as_str(), &[]);

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // Current price is 1.216736524026807943
    // so ret_amount = 1_500_000_000 * 1.216736524026807943
    let expected_ret_amount = Uint128::new(1_825_104_786u128);

    // no spread
    let expected_spread_amount = Uint128::new(0u128);

    let expected_commission_amount = Uint128::new(0u128);
    let expected_maker_fee_amount = Uint128::new(0u128);
    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();
    let expected_tax_amount = std::cmp::min(
        Uint128::new(1000000u128),
        expected_return_amount
            .checked_sub(
                expected_return_amount.multiply_ratio(Uint128::new(100u128), Uint128::new(101u128)),
            )
            .unwrap(),
    );
    // Check simulation res
    // Return asset token balance as normal
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &(asset_pool_amount))],
    )]);

    let simulation_res: SimulationResponse = query_simulation(
        deps.as_ref(),
        Asset {
            amount: offer_amount,
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
        },
    )
    .unwrap();
    assert_eq!(expected_return_amount, simulation_res.return_amount);
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert_eq!(expected_spread_amount, simulation_res.spread_amount);

    // Check reverse simulation result
    let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            amount: expected_return_amount,
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
    )
    .unwrap();
    assert!(
        (offer_amount.u128() as i128 - reverse_simulation_res.offer_amount.u128() as i128).abs()
            < 5i128
    );
    assert!(
        (expected_commission_amount.u128() as i128
            - reverse_simulation_res.commission_amount.u128() as i128)
            .abs()
            < 5i128
    );
    assert!(
        (expected_spread_amount.u128() as i128
            - reverse_simulation_res.spread_amount.u128() as i128)
            .abs()
            < 5i128
    );

    let first_msg = res.messages.get(0).unwrap();

    match &first_msg.msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_ANCHOR_TOKEN);
            assert!(funds.is_empty());
            match from_binary(msg).unwrap() {
                Cw20ExecuteMsg::Send {
                    contract,
                    amount,
                    msg,
                } => {
                    assert_eq!(contract, MOCK_ANCHOR_ADDR);
                    assert_eq!(amount, offer_amount);
                    let redeem_msg: AnchorCw20HookMsg = from_binary(&msg).unwrap();
                    assert_eq!(redeem_msg, AnchorCw20HookMsg::RedeemStable {})
                }
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let second_msg = res.messages.get(1).unwrap();
    let sub_msg: ExecuteMsg;

    match &second_msg.msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_CONTRACT_ADDR);
            assert!(funds.is_empty());
            sub_msg = from_binary(&msg).unwrap();

            assert_eq!(
                sub_msg,
                ExecuteMsg::AssertAndSend {
                    receiver: Addr::unchecked(sender),
                    sender: Addr::unchecked(sender),
                    offer_asset: Asset {
                        info: AssetInfo::Token {
                            contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN)
                        },
                        amount: offer_amount,
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uusd".to_string()
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                }
            );
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    assert_eq!(res.attributes, vec![attr("action", "orchestrate"),]);

    // apply anchor redeem
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: expected_return_amount,
        }],
    )]);

    let second_response =
        execute(deps.as_mut(), env.clone().clone(), info_contract, sub_msg).unwrap();
    let msg_transfer = second_response.messages.get(0).expect("no message");

    assert_eq!(
        second_response.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", MOCK_ANCHOR_TOKEN),
            attr("ask_asset", "uusd"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
            attr("tax_amount", expected_tax_amount.to_string()),
            attr("spread_amount", expected_spread_amount.to_string()),
            attr("commission_amount", expected_commission_amount.to_string()),
            attr("maker_fee_amount", expected_maker_fee_amount.to_string()),
        ]
    );

    assert_eq!(
        msg_transfer,
        &SubMsg {
            msg: CosmosMsg::Bank(BankMsg::Send {
                to_address: String::from("addr0000"),
                amount: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: expected_return_amount
                        .checked_sub(expected_tax_amount)
                        .unwrap(),
                }],
            })
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
    );

    // Failed due to trying to swap a non token (specifying an address of a non token contract)
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info("liquidtity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});
}

#[test]
fn test_max_spread() {
    assert_max_spread(
        Some(Decimal::from_ratio(1200u128, 1u128)),
        Some(Decimal::percent(1)),
        Uint128::from(1200000000u128),
        Uint128::from(989999u128),
    )
    .unwrap_err();

    assert_max_spread(
        Some(Decimal::from_ratio(1200u128, 1u128)),
        Some(Decimal::percent(1)),
        Uint128::from(1200000000u128),
        Uint128::from(990000u128),
    )
    .unwrap();

    assert_max_spread(
        None,
        Some(Decimal::percent(1)),
        Uint128::zero(),
        Uint128::from(990000u128),
    )
    .unwrap();
}

#[test]
fn test_deduct() {
    let mut deps = mock_dependencies(&[]);

    let tax_rate = Decimal::percent(2);
    let tax_cap = Uint128::from(1_000_000u128);
    deps.querier.with_tax(
        Decimal::percent(2),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    let amount = Uint128::new(1000_000_000u128);
    let expected_after_amount = std::cmp::max(
        amount.checked_sub(amount * tax_rate).unwrap(),
        amount.checked_sub(tax_cap).unwrap(),
    );

    let after_amount = (Asset {
        info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
        amount,
    })
    .deduct_tax(&deps.as_ref().querier)
    .unwrap();

    assert_eq!(expected_after_amount, after_amount.amount);
}

#[test]
fn test_query_pool() {
    let total_share_amount = Uint128::from(0u128);
    let asset_0_amount = Uint128::from(0u128);
    let asset_1_amount = Uint128::from(0u128);
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    deps.querier.with_token_balances(&[(
        &String::from(MOCK_ANCHOR_TOKEN),
        &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let res: PoolResponse = query_pool(deps.as_ref()).unwrap();

    assert_eq!(
        res.assets,
        [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: asset_0_amount
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
                },
                amount: asset_1_amount
            }
        ]
    );
    assert_eq!(res.total_share, total_share_amount);
}

#[test]
fn test_query_share() {
    let asset_0_amount = Uint128::from(250u128);
    let asset_1_amount = Uint128::from(1000u128);
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    deps.querier.with_token_balances(&[(
        &String::from(MOCK_ANCHOR_TOKEN),
        &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let res = query_share();

    assert_eq!(res.len(), 0);
}

#[test]
fn test_sending_aust_balance_to_maker() {
    let asset_pool_amount = Uint128::new(0u128);
    let aust_pool_amount = Uint128::new(1234u128);
    let collateral_pool_amount = Uint128::new(0u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount + offer_amount, /* user deposit must be pre-applied */
    }]);

    deps.querier.with_tax(
        Decimal::zero(),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    deps.querier.with_token_balances(&[
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &asset_pool_amount)],
        ),
        (
            &String::from(MOCK_ANCHOR_TOKEN),
            &[(&String::from(MOCK_CONTRACT_ADDR), &aust_pool_amount)],
        ),
    ]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let info_contract = mock_info(env.contract.address.as_str(), &[]);
    // we can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Normal swap
    let execute_msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: offer_amount,
        },
        belief_price: None,
        max_spread: Some(Decimal::percent(50)),
        to: None,
    };
    let env = mock_env_with_block_time(1000);
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: offer_amount,
        }],
    );

    let res = execute(deps.as_mut(), env.clone(), info.clone(), execute_msg).unwrap();

    // Current price is 1.216736524026807943
    // so ret_amount = 1_500_000_000 / 1.216736524026807943
    let expected_ret_amount = Uint128::new(1_232_805_928u128);

    // no spread
    let expected_spread_amount = Uint128::new(0);
    let expected_commission_amount = Uint128::new(0);

    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();

    // Check simulation result
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: collateral_pool_amount, /* user deposit must be pre-applied */
        }],
    )]);

    let simulation_res: SimulationResponse = query_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: offer_amount,
        },
    )
    .unwrap();
    assert_eq!(expected_return_amount, simulation_res.return_amount);
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert_eq!(expected_spread_amount, simulation_res.spread_amount);

    // checks that the existing assets are moved to the maker contract
    let first_msg = res.messages.get(0).unwrap();

    match first_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_ANCHOR_TOKEN);
            assert_eq!(funds, vec![]);
            let sub_msg: Cw20ExecuteMsg = from_binary(&msg).unwrap();

            assert_eq!(
                sub_msg,
                Cw20ExecuteMsg::Transfer {
                    amount: aust_pool_amount,
                    recipient: "fee_address".to_string(),
                }
            );
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let second_msg = res.messages.get(1).unwrap();

    match second_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_ANCHOR_ADDR);
            assert_eq!(funds, info.funds);
            match from_binary(&msg).unwrap() {
                AnchorExecuteMsg::DepositStable {} => {}
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let third_msg = res.messages.get(2).unwrap();
    let sub_msg: ExecuteMsg;

    match third_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_CONTRACT_ADDR);
            assert_eq!(funds, vec![]);
            sub_msg = from_binary(&msg).unwrap();

            assert_eq!(
                sub_msg,
                ExecuteMsg::AssertAndSend {
                    receiver: info.sender.clone(),
                    sender: info.sender,
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: offer_amount,
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN)
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                }
            );
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    assert_eq!(res.attributes, vec![attr("action", "orchestrate"),]);

    // apply anchor deposit
    deps.querier.with_token_balances(&[(
        &String::from(MOCK_ANCHOR_TOKEN),
        &[(&String::from(MOCK_CONTRACT_ADDR), &expected_return_amount)],
    )]);

    let second_response =
        execute(deps.as_mut(), env.clone().clone(), info_contract, sub_msg).unwrap();
    let msg_transfer = second_response.messages.get(0).expect("no message");

    assert_eq!(
        second_response.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "uusd"),
            attr("ask_asset", MOCK_ANCHOR_TOKEN),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_ret_amount.to_string()),
            attr("tax_amount", "0"),
            attr("spread_amount", "0"),
            attr("commission_amount", "0"),
            attr("maker_fee_amount", "0"),
        ]
    );

    assert_eq!(
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from(MOCK_ANCHOR_TOKEN),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: expected_ret_amount,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
        msg_transfer,
    );
}

#[test]
fn test_sending_ust_balance_to_maker() {
    let asset_pool_amount = Uint128::new(0u128);
    let uusd_pool_amount = Uint128::new(1234u128);
    let collateral_pool_amount = Uint128::new(0u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount,
    }]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: uusd_pool_amount,
        }],
    )]);
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &(collateral_pool_amount))],
    )]);

    let msg = get_instantiate_message();

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Unauthorized access; can not execute swap directly for token swap
    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
            amount: offer_amount,
        },
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let sender = "addr0000";
    let env = mock_env_with_block_time(1000);
    let info = mock_info(sender, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Normal sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from(sender),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            belief_price: None,
            max_spread: Some(Decimal::percent(50)),
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info(MOCK_ANCHOR_TOKEN, &[]);
    let info_contract = mock_info(env.contract.address.clone().as_str(), &[]);

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // Current price is 1.216736524026807943
    // so ret_amount = 1_500_000_000 * 1.216736524026807943
    let expected_ret_amount = Uint128::new(1_825_104_786u128);

    // no spread
    let expected_spread_amount = Uint128::new(0u128);

    let expected_commission_amount = Uint128::new(0u128);
    let expected_maker_fee_amount = Uint128::new(0u128);
    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();
    let expected_tax_amount = std::cmp::min(
        Uint128::new(1000000u128),
        expected_return_amount
            .checked_sub(
                expected_return_amount.multiply_ratio(Uint128::new(100u128), Uint128::new(101u128)),
            )
            .unwrap(),
    );
    // Check simulation res
    // Return asset token balance as normal
    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &(asset_pool_amount))],
    )]);

    let simulation_res: SimulationResponse = query_simulation(
        deps.as_ref(),
        Asset {
            amount: offer_amount,
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
        },
    )
    .unwrap();
    assert_eq!(expected_return_amount, simulation_res.return_amount);
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert_eq!(expected_spread_amount, simulation_res.spread_amount);

    // Check reverse simulation result
    let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            amount: expected_return_amount,
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        (offer_amount.u128() as i128 - reverse_simulation_res.offer_amount.u128() as i128).abs()
            < 5i128,
        true
    );
    assert_eq!(
        (expected_commission_amount.u128() as i128
            - reverse_simulation_res.commission_amount.u128() as i128)
            .abs()
            < 5i128,
        true
    );
    assert_eq!(
        (expected_spread_amount.u128() as i128
            - reverse_simulation_res.spread_amount.u128() as i128)
            .abs()
            < 5i128,
        true
    );

    // checks that the existing assets (uusd) are moved to the maker contract
    let first_msg = res.messages.get(0).unwrap();

    assert_eq!(
        first_msg.msg.clone(),
        CosmosMsg::Bank(BankMsg::Send {
            to_address: String::from("fee_address"),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: uusd_pool_amount
                    .checked_sub(
                        // tax amount
                        Uint128::from(13u128)
                    )
                    .unwrap()
            }],
        })
    );

    let second_msg = res.messages.get(1).unwrap();

    match second_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_ANCHOR_TOKEN);
            assert_eq!(funds, vec![]);
            match from_binary(&msg).unwrap() {
                Cw20ExecuteMsg::Send {
                    contract,
                    amount,
                    msg,
                } => {
                    assert_eq!(contract, MOCK_ANCHOR_ADDR);
                    assert_eq!(amount, offer_amount);
                    let redeem_msg: AnchorCw20HookMsg = from_binary(&msg).unwrap();
                    assert_eq!(redeem_msg, AnchorCw20HookMsg::RedeemStable {})
                }
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let third_msg = res.messages.get(2).unwrap();
    let sub_msg: ExecuteMsg;

    match third_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            funds,
            contract_addr,
            msg,
        }) => {
            assert_eq!(contract_addr, MOCK_CONTRACT_ADDR);
            assert_eq!(funds, vec![]);
            sub_msg = from_binary(&msg).unwrap();

            assert_eq!(
                sub_msg,
                ExecuteMsg::AssertAndSend {
                    receiver: Addr::unchecked(sender),
                    sender: Addr::unchecked(sender),
                    offer_asset: Asset {
                        info: AssetInfo::Token {
                            contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN)
                        },
                        amount: offer_amount,
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uusd".to_string()
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                }
            );
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    assert_eq!(res.attributes, vec![attr("action", "orchestrate"),]);

    // apply anchor redeem
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: expected_return_amount,
        }],
    )]);

    let second_response =
        execute(deps.as_mut(), env.clone().clone(), info_contract, sub_msg).unwrap();
    let msg_transfer = second_response.messages.get(0).expect("no message");

    assert_eq!(
        second_response.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", MOCK_ANCHOR_TOKEN),
            attr("ask_asset", "uusd"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
            attr("tax_amount", expected_tax_amount.to_string()),
            attr("spread_amount", expected_spread_amount.to_string()),
            attr("commission_amount", expected_commission_amount.to_string()),
            attr("maker_fee_amount", expected_maker_fee_amount.to_string()),
        ]
    );

    assert_eq!(
        &SubMsg {
            msg: CosmosMsg::Bank(BankMsg::Send {
                to_address: String::from("addr0000"),
                amount: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: expected_return_amount
                        .checked_sub(expected_tax_amount)
                        .unwrap(),
                }],
            })
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
        msg_transfer,
    );

    // Failed due to trying to swap a non token (specifying an address of a non token contract)
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info("liquidtity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});
}

fn mock_env_with_block_time(time: u64) -> Env {
    let mut env = mock_env();
    env.block = BlockInfo {
        height: 1,
        time: Timestamp::from_seconds(time),
        chain_id: "columbus".to_string(),
    };
    env
}

fn get_instantiate_message() -> InstantiateMsg {
    InstantiateMsg {
        factory_addr: String::from("factory"),
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked(MOCK_ANCHOR_TOKEN),
            },
        ],
        init_params: create_init_params(),
        token_code_id: 0u64,
    }
}
