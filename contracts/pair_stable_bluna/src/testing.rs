use crate::contract::{
    accumulate_prices, assert_max_spread, calc_user_reward, compute_offer_amount, compute_swap,
    execute, instantiate, query_pair_info, query_pool, query_reverse_simulation, query_share,
    query_simulation, reply,
};
use crate::error::ContractError;
use crate::math::{calc_ask_amount, calc_offer_amount, AMP_PRECISION};
use crate::mock_querier::mock_dependencies;
use crate::response::MsgInstantiateContractResponse;
use crate::state::Config;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::{
    Cw20HookMsg, InstantiateMsg, PoolResponse, ReverseSimulationResponse, SimulationResponse,
    TWAP_PRECISION,
};
use astroport::pair_stable_bluna::{ExecuteMsg, StablePoolParams};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, to_binary, Addr, BankMsg, BlockInfo, Coin, CosmosMsg, Decimal, Decimal256, DepsMut, Env,
    Reply, ReplyOn, StdError, SubMsg, SubMsgResponse, SubMsgResult, Timestamp, Uint128, WasmMsg,
};
use cw1_whitelist::msg::InstantiateMsg as WhitelistInstantiateMsg;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use protobuf::Message;
use std::str::FromStr;

fn store_liquidity_token(deps: DepsMut, msg_id: u64, contract_addr: String) {
    let data = MsgInstantiateContractResponse {
        contract_address: contract_addr,
        data: vec![],
        unknown_fields: Default::default(),
        cached_size: Default::default(),
    }
    .write_to_bytes()
    .unwrap();

    let reply_msg = Reply {
        id: msg_id,
        result: SubMsgResult::Ok {
            0: SubMsgResponse {
                events: vec![],
                data: Some(data.into()),
            },
        },
    };

    reply(deps, mock_env(), reply_msg.clone()).unwrap();
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128))],
    )]);

    let msg = InstantiateMsg {
        factory_addr: "factory".to_string(),
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let sender = "addr0000";
    // We can just call .unwrap() to assert this was a success
    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                msg: WasmMsg::Instantiate {
                    code_id: 666u64,
                    msg: to_binary(&WhitelistInstantiateMsg {
                        admins: vec![env.contract.address.to_string()],
                        mutable: false,
                    })
                    .unwrap(),
                    funds: vec![],
                    admin: None,
                    label: String::from("Bluna rewarder"),
                }
                .into(),
                id: 2,
                gas_limit: None,
                reply_on: ReplyOn::Success
            },
            SubMsg {
                msg: WasmMsg::Instantiate {
                    code_id: 10u64,
                    msg: to_binary(&TokenInstantiateMsg {
                        name: "UUSD-MAPP-LP".to_string(),
                        symbol: "uLP".to_string(),
                        decimals: 6,
                        initial_balances: vec![],
                        mint: Some(MinterResponse {
                            minter: String::from(MOCK_CONTRACT_ADDR),
                            cap: None,
                        }),
                        marketing: None
                    })
                    .unwrap(),
                    funds: vec![],
                    admin: None,
                    label: String::from("Astroport LP token"),
                }
                .into(),
                id: 1,
                gas_limit: None,
                reply_on: ReplyOn::Success
            }
        ]
    );

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // It worked, let's query the state
    let pair_info: PairInfo = query_pair_info(deps.as_ref()).unwrap();
    assert_eq!(Addr::unchecked("liquidity0000"), pair_info.liquidity_token);
    assert_eq!(
        pair_info.asset_infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000")
            }
        ]
    );
}

#[test]
fn provide_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(200_000000000000000000u128),
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(0))],
        ),
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(0))],
        ),
        (
            &String::from("bluna_rewarder"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(1000))],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // Successfully provide liquidity for the existing pool
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
    let res = execute(deps.as_mut(), env.clone().clone(), info, msg).unwrap();
    let transfer_from_msg = res.messages.get(0).expect("no message");
    let mint_msg = res.messages.get(1).expect("no message");
    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset0000"),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: String::from("addr0000"),
                    recipient: String::from(MOCK_CONTRACT_ADDR),
                    amount: Uint128::from(100_000000000000000000u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never
        }
    );
    assert_eq!(
        mint_msg,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("liquidity0000"),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(100_000000000000000000u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    // Check wrong argument
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
                amount: Uint128::from(50_000000000000000000u128),
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
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    match res {
        ContractError::Std(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            "Native token balance mismatch between the argument and the transferred".to_string()
        ),
        _ => panic!("Must return generic error"),
    }
}

#[test]
fn withdraw_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(100u128),
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(&String::from("addr0000"), &Uint128::new(0))],
        ),
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(100u128))],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // Withdraw liquidity
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        msg: to_binary(&Cw20HookMsg::WithdrawLiquidity { assets: vec![] }).unwrap(),
        amount: Uint128::new(100u128),
    });

    let env = mock_env();
    let info = mock_info("liquidity0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let log_withdrawn_share = res.attributes.get(2).expect("no log");
    let log_refund_assets = res.attributes.get(3).expect("no log");
    let msg_refund_0 = res.messages.get(0).expect("no message");
    let msg_refund_1 = res.messages.get(1).expect("no message");
    let msg_burn_liquidity = res.messages.get(2).expect("no message");
    assert_eq!(
        msg_refund_0,
        &SubMsg {
            msg: CosmosMsg::Bank(BankMsg::Send {
                to_address: String::from("addr0000"),
                amount: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(0u128),
                }],
            }),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );
    assert_eq!(
        msg_refund_1,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset0000"),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(0u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );
    assert_eq!(
        msg_burn_liquidity,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("liquidity0000"),
                msg: to_binary(&Cw20ExecuteMsg::Burn {
                    amount: Uint128::from(100u128),
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    assert_eq!(
        log_withdrawn_share,
        &attr("withdrawn_share", 100u128.to_string())
    );
    assert_eq!(
        log_refund_assets,
        &attr("refund_assets", "0uusd, 0asset0000")
    );
}

#[test]
fn try_native_to_token() {
    let total_share = Uint128::new(30000000000u128);
    let asset_pool_amount = Uint128::new(20000000000u128);
    let collateral_pool_amount = Uint128::new(30000000000u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount + offer_amount, /* user deposit must be pre-applied */
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &total_share)],
        ),
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &asset_pool_amount)],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env_with_block_time(100);
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // Normal swap
    let msg = ExecuteMsg::Swap {
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

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    let model: StableSwapModel = StableSwapModel::new(
        100,
        vec![collateral_pool_amount.into(), asset_pool_amount.into()],
        2,
    );

    let sim_result = model.sim_exchange(0, 1, offer_amount.into());

    let expected_ret_amount = Uint128::new(sim_result);
    let expected_spread_amount = offer_amount.saturating_sub(expected_ret_amount);
    let expected_commission_amount = expected_ret_amount.multiply_ratio(3u128, 1000u128); // 0.3%
    let expected_maker_fee_amount = expected_commission_amount.multiply_ratio(166u128, 1000u128);

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
        env.clone(),
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

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "uusd"),
            attr("ask_asset", "asset0000"),
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
                contract_addr: String::from("asset0000"),
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
    let total_share = Uint128::new(30000000000u128);
    let asset_pool_amount = Uint128::new(20000000000u128);
    let collateral_pool_amount = Uint128::new(30000000000u128);
    let offer_amount = Uint128::new(1500000000u128);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: collateral_pool_amount,
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &total_share)],
        ),
        (
            &String::from("asset0000"),
            &[(
                &String::from(MOCK_CONTRACT_ADDR),
                &(asset_pool_amount + offer_amount),
            )],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env_with_block_time(100);
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // Unauthorized access; can not execute swap directy for token swap
    let msg = ExecuteMsg::Swap {
        offer_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
            amount: offer_amount,
        },
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let env = mock_env_with_block_time(1000);
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Normal sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            ask_asset_info: None,
            belief_price: None,
            max_spread: None,
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info("asset0000", &[]);

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    let model: StableSwapModel = StableSwapModel::new(
        100,
        vec![collateral_pool_amount.into(), asset_pool_amount.into()],
        2,
    );

    let sim_result = model.sim_exchange(1, 0, offer_amount.into());

    let expected_ret_amount = Uint128::new(sim_result);
    let expected_spread_amount = offer_amount.saturating_sub(expected_ret_amount);
    let expected_commission_amount = expected_ret_amount.multiply_ratio(3u128, 1000u128); // 0.3%
    let expected_maker_fee_amount = expected_commission_amount.multiply_ratio(166u128, 1000u128);

    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();

    // Check simulation res
    // Return asset token balance
    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &total_share)],
        ),
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &(asset_pool_amount))],
        ),
    ]);

    let simulation_res: SimulationResponse = query_simulation(
        deps.as_ref(),
        env.clone(),
        Asset {
            amount: offer_amount,
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
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
        env,
        Asset {
            amount: expected_return_amount,
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
    )
    .unwrap();

    let sim_result = model.sim_exchange(0, 1, expected_ret_amount.into());

    let reverse_expected_spread_amount =
        Uint128::new(sim_result).saturating_sub(expected_ret_amount);

    assert_eq!(
        (offer_amount.u128() as i128 - reverse_simulation_res.offer_amount.u128() as i128).abs()
            < 5i128,
        true
    );
    assert_eq!(
        expected_commission_amount,
        reverse_simulation_res.commission_amount
    );
    assert_eq!(
        reverse_expected_spread_amount,
        reverse_simulation_res.spread_amount
    );

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "asset0000"),
            attr("ask_asset", "uusd"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", expected_return_amount.to_string()),
            attr("tax_amount", Uint128::zero().to_string()),
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
                    amount: expected_return_amount,
                }],
            })
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
        msg_transfer,
    );

    // Failed due to trying to swap with a non asset token contract
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_binary(&Cw20HookMsg::Swap {
            ask_asset_info: None,
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
        Uint128::zero(),
    )
    .unwrap_err();

    assert_max_spread(
        Some(Decimal::from_ratio(1200u128, 1u128)),
        Some(Decimal::percent(1)),
        Uint128::from(1200000000u128),
        Uint128::from(990000u128),
        Uint128::zero(),
    )
    .unwrap();

    assert_max_spread(
        None,
        Some(Decimal::percent(1)),
        Uint128::zero(),
        Uint128::from(989999u128),
        Uint128::from(10001u128),
    )
    .unwrap_err();

    assert_max_spread(
        None,
        Some(Decimal::percent(1)),
        Uint128::zero(),
        Uint128::from(990000u128),
        Uint128::from(10000u128),
    )
    .unwrap();
}

#[test]
fn test_query_pool() {
    let total_share_amount = Uint128::from(111u128);
    let asset_0_amount = Uint128::from(222u128);
    let asset_1_amount = Uint128::from(333u128);
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
        ),
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &total_share_amount)],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

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
                    contract_addr: Addr::unchecked("asset0000"),
                },
                amount: asset_1_amount
            }
        ]
    );
    assert_eq!(res.total_share, total_share_amount);
}

#[test]
fn test_query_share() {
    let total_share_amount = Uint128::from(500u128);
    let asset_0_amount = Uint128::from(250u128);
    let asset_1_amount = Uint128::from(1000u128);
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    deps.querier.with_token_balances(&[
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
        ),
        (
            &String::from("liquidity0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &total_share_amount)],
        ),
    ]);

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
            },
        ],
        token_code_id: 10u64,
        factory_addr: "factory".to_string(),
        init_params: Some(
            to_binary(&StablePoolParams {
                amp: 100,
                bluna_rewarder: "bluna_rewarder".to_string(),
                generator: "generator".to_string(),
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    let res = query_share(deps.as_ref(), Uint128::new(250)).unwrap();

    assert_eq!(res[0].amount, Uint128::new(125));
    assert_eq!(res[1].amount, Uint128::new(500));
}

#[test]
fn test_accumulate_prices() {
    struct Case {
        block_time: u64,
        block_time_last: u64,
        last0: u128,
        last1: u128,
        x_amount: u128,
        y_amount: u128,
    }

    struct Result {
        block_time_last: u64,
        cumulative_price_x: u128,
        cumulative_price_y: u128,
        is_some: bool,
    }

    let price_precision = 10u128.pow(TWAP_PRECISION.into());

    let test_cases: Vec<(Case, Result)> = vec![
        (
            Case {
                block_time: 1000,
                block_time_last: 0,
                last0: 0,
                last1: 0,
                x_amount: 250_000000,
                y_amount: 500_000000,
            },
            Result {
                block_time_last: 1000,
                cumulative_price_x: 1008,
                cumulative_price_y: 991,
                is_some: true,
            },
        ),
        // Same block height, no changes
        (
            Case {
                block_time: 1000,
                block_time_last: 1000,
                last0: 1 * price_precision,
                last1: 2 * price_precision,
                x_amount: 250_000000,
                y_amount: 500_000000,
            },
            Result {
                block_time_last: 1000,
                cumulative_price_x: 1,
                cumulative_price_y: 2,
                is_some: false,
            },
        ),
        (
            Case {
                block_time: 1500,
                block_time_last: 1000,
                last0: 500 * price_precision,
                last1: 2000 * price_precision,
                x_amount: 250_000000,
                y_amount: 500_000000,
            },
            Result {
                block_time_last: 1500,
                cumulative_price_x: 1004,
                cumulative_price_y: 2495,
                is_some: true,
            },
        ),
    ];

    for test_case in test_cases {
        let (case, result) = test_case;

        let env = mock_env_with_block_time(case.block_time);
        let config = accumulate_prices(
            env.clone(),
            &Config {
                pair_info: PairInfo {
                    asset_infos: vec![
                        AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        AssetInfo::Token {
                            contract_addr: Addr::unchecked("asset0000"),
                        },
                    ],
                    contract_addr: Addr::unchecked("pair"),
                    liquidity_token: Addr::unchecked("lp_token"),
                    pair_type: PairType::Stable {},
                },
                factory_addr: Addr::unchecked("factory"),
                block_time_last: case.block_time_last,
                price0_cumulative_last: Uint128::new(case.last0),
                price1_cumulative_last: Uint128::new(case.last1),
                init_amp: 100 * AMP_PRECISION,
                init_amp_time: env.block.time.seconds(),
                next_amp: 100 * AMP_PRECISION,
                next_amp_time: env.block.time.seconds(),
                bluna_rewarder: Addr::unchecked(""),
                generator: Addr::unchecked("generator"),
            },
            Uint128::new(case.x_amount),
            6,
            Uint128::new(case.y_amount),
            6,
        )
        .unwrap();

        assert_eq!(result.is_some, config.is_some());

        if let Some(config) = config {
            assert_eq!(config.2, result.block_time_last);
            assert_eq!(
                config.0 / Uint128::from(price_precision),
                Uint128::new(result.cumulative_price_x)
            );
            assert_eq!(
                config.1 / Uint128::from(price_precision),
                Uint128::new(result.cumulative_price_y)
            );
        }
    }
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

#[test]
fn test_calc_user_reward() {
    // Check overflow error
    calc_user_reward(
        Uint128::new(10000),
        Uint128::new(1000),
        Uint128::new(1000000000000000000000000000),
        Uint128::new(1000000000000000000000000000),
        Decimal256::from_str("100000000000000000000000000").unwrap(),
        Some(Decimal256::from_str("100").unwrap()),
    )
    .unwrap_err();

    // All rewards are awarded to one user
    let (bluna_reward_global_index, latest_reward_amount, user_reward) = calc_user_reward(
        Uint128::new(10000),
        Uint128::new(1000),
        Uint128::new(100),
        Uint128::new(100),
        Decimal256::from_str("100").unwrap(),
        Some(Decimal256::from_str("100").unwrap()),
    )
    .unwrap();
    assert_eq!(
        Decimal256::from_str("190").unwrap(),
        bluna_reward_global_index
    );
    assert_eq!(Uint128::new(9000), latest_reward_amount);
    assert_eq!(Uint128::new(9000), user_reward);

    // Only 10% of the reward is given to the user
    let (bluna_reward_global_index, latest_reward_amount, user_reward) = calc_user_reward(
        Uint128::new(10000),
        Uint128::new(1000),
        Uint128::new(10),
        Uint128::new(100),
        Decimal256::from_str("100").unwrap(),
        Some(Decimal256::from_str("100").unwrap()),
    )
    .unwrap();
    assert_eq!(
        Decimal256::from_str("190").unwrap(),
        bluna_reward_global_index
    );
    assert_eq!(Uint128::new(9000), latest_reward_amount);
    assert_eq!(Uint128::new(900), user_reward);
}

use astroport::factory::PairType;
use proptest::prelude::*;
use sim::StableSwapModel;

proptest! {
    #[test]
    fn constant_product_swap_no_fee(
        balance_in in 100..1_000_000_000_000_000_000u128,
        balance_out in 100..1_000_000_000_000_000_000u128,
        amount_in in 100..100_000_000_000u128,
        amp in 1..150u64
    ) {
        prop_assume!(amount_in < balance_in);

        let model: StableSwapModel = StableSwapModel::new(
            amp.into(),
            vec![balance_in, balance_out],
            2,
        );

        let result = calc_ask_amount(
            balance_in,
            balance_out,
            amount_in,
            amp * AMP_PRECISION
        ).unwrap();

        let sim_result = model.sim_exchange(0, 1, amount_in);

        let diff = (sim_result as i128 - result as i128).abs();

        assert!(
            diff <= 1,
            "result={}, sim_result={}, amp={}, amount_in={}, balance_in={}, balance_out={}, diff={}",
            result,
            sim_result,
            amp,
            amount_in,
            balance_in,
            balance_out,
            diff
        );

        let reverse_result = calc_offer_amount(
            balance_in,
            balance_out,
            result,
            amp * AMP_PRECISION
        ).unwrap();

        let amount_in_f = amount_in as f64;
        let reverse_diff = (reverse_result as f64 - amount_in_f) / amount_in_f * 100.;

        assert!(
            reverse_diff <= 0.0001,
            "result={}, sim_result={}, amp={}, amount_out={}, balance_in={}, balance_out={}, diff(%)={}",
            reverse_result,
            amount_in,
            amp,
            result,
            balance_in,
            balance_out,
            reverse_diff
        );
    }
}

#[test]
fn ensure_useful_error_messages_are_given_on_swaps() {
    const OFFER: Uint128 = Uint128::new(1_000_000_000000);
    const ASK: Uint128 = Uint128::new(1_000_000_000000);
    const AMOUNT: Uint128 = Uint128::new(1_000000);
    const ZERO: Uint128 = Uint128::zero();
    const DZERO: Decimal = Decimal::zero();
    const AMP: u64 = 100;
    const PRS: u8 = 6;

    // Computing ask
    assert_eq!(
        compute_swap(ZERO, PRS, ZERO, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, PRS, ZERO, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, PRS, ASK, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, PRS, ASK, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, PRS, ZERO, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, PRS, ZERO, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, PRS, ASK, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("Swap amount must not be zero")
    );
    compute_swap(OFFER, PRS, ASK, PRS, AMOUNT, DZERO, AMP).unwrap();

    // Computing offer
    assert_eq!(
        compute_offer_amount(ZERO, PRS, ZERO, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, PRS, ZERO, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, PRS, ASK, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, PRS, ASK, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, PRS, ZERO, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, PRS, ZERO, PRS, AMOUNT, DZERO, AMP).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, PRS, ASK, PRS, ZERO, DZERO, AMP).unwrap_err(),
        StdError::generic_err("Swap amount must not be zero")
    );
    compute_offer_amount(OFFER, PRS, ASK, PRS, AMOUNT, DZERO, AMP).unwrap();
}
