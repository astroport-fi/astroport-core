use std::error::Error;
use std::str::FromStr;

use astroport::token_factory::{MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, coin, from_json, to_json_binary, Addr, BankMsg, Binary, BlockInfo, Coin, CosmosMsg,
    Decimal, DepsMut, Env, Reply, ReplyOn, Response, SubMsg, SubMsgResponse, SubMsgResult,
    Timestamp, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use itertools::Itertools;
use proptest::prelude::*;
use prost::Message;
use sim::StableSwapModel;

use astroport::asset::{native_asset, native_asset_info, Asset, AssetInfo};
use astroport::observation::query_observation;
use astroport::observation::Observation;
use astroport::observation::OracleObservation;
use astroport::pair::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolResponse, QueryMsg,
    SimulationResponse, StablePoolParams,
};
use astroport_circular_buffer::BufferManager;

use crate::contract::{
    assert_max_spread, execute, instantiate, query, query_pool, query_reverse_simulation,
    query_share, query_simulation, reply, LP_SUBDENOM,
};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::{CONFIG, OBSERVATIONS};
use crate::utils::{compute_swap, select_pools};

#[derive(Clone, PartialEq, Message)]
struct MsgInstantiateContractResponse {
    #[prost(string, tag = "1")]
    pub contract_address: String,
    #[prost(bytes, tag = "2")]
    pub data: Vec<u8>,
}

fn store_liquidity_token(deps: DepsMut, msg_id: u64, subdenom: String) {
    let reply_msg = Reply {
        id: msg_id,
        result: SubMsgResult::Ok(SubMsgResponse {
            events: vec![],
            data: Some(
                MsgCreateDenomResponse {
                    new_token_denom: subdenom,
                }
                .into(),
            ),
        }),
    };

    reply(deps, mock_env(), reply_msg).unwrap();
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(123u128))],
    )]);

    let msg = InstantiateMsg {
        factory_addr: String::from("factory"),
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
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
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
        vec![SubMsg {
            msg: CosmosMsg::Stargate {
                type_url: "/osmosis.tokenfactory.v1beta1.MsgCreateDenom".to_string(),
                value: Binary(
                    MsgCreateDenom {
                        sender: env.contract.address.to_string(),
                        subdenom: LP_SUBDENOM.to_string()
                    }
                    .encode_to_vec()
                )
            },
            id: 1,
            gas_limit: None,
            reply_on: ReplyOn::Success
        },]
    );

    let denom = format!("factory/{}/{}", env.contract.address, "astroport/share");

    // Store liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

    // It worked, let's query the state
    let pair_info = CONFIG.load(deps.as_ref().storage).unwrap().pair_info;
    assert_eq!(denom, pair_info.liquidity_token);
    assert_eq!(
        pair_info.asset_infos,
        vec![
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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();
    let denom = format!("factory/{}/{}", env.contract.address, "share/astroport");

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

    // Successfully provide liquidity for the existing pool
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
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
    let mint_min_liquidity_msg = res.messages.get(1).expect("no message");
    let mint_receiver_msg = res.messages.get(2).expect("no message");

    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset0000"),
                msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
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
        mint_min_liquidity_msg,
        &SubMsg {
            msg: CosmosMsg::Stargate {
                type_url: "/osmosis.tokenfactory.v1beta1.MsgMint".to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(1000_u128).to_string(),
                        }),

                        mint_to_address: String::from(MOCK_CONTRACT_ADDR),
                        sender: env.contract.address.to_string(),
                    }
                    .encode_to_vec()
                )
            },
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    assert_eq!(
        mint_receiver_msg,
        &SubMsg {
            msg: CosmosMsg::Stargate {
                type_url: "/osmosis.tokenfactory.v1beta1.MsgMint".to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(299_814_698_523_989_456_628u128).to_string(),
                        }),

                        mint_to_address: String::from("addr0000"),
                        sender: env.contract.address.to_string(),
                    }
                    .encode_to_vec()
                )
            },
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    // Provide more liquidity using a 1:2 ratio
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(200_000000000000000000 + 200_000000000000000000 /* user deposit must be pre-applied */),
        }],
        
    ),
    (
        &String::from("liquidity0000"),
        &[coin(100_000000000000000000u128, denom.to_string())],
    ),
    ]);

    deps.querier.with_token_balances(&[
       
        (
            &String::from("asset0000"),
            &[(
                &String::from(MOCK_CONTRACT_ADDR),
                &Uint128::new(200_000000000000000000),
            )],
        ),
    ]);

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
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
                amount: Uint128::from(200_000000000000000000u128),
            },
        ],
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(200_000000000000000000u128),
        }],
    );

    let res: Response = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    let transfer_from_msg = res.messages.get(0).expect("no message");
    let mint_msg = res.messages.get(1).expect("no message");

    assert_eq!(
        transfer_from_msg,
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset0000"),
                msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
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
            reply_on: ReplyOn::Never,
        }
    );
    assert_eq!(
        mint_msg,
        &SubMsg {
            msg: CosmosMsg::Stargate {
                type_url: "/osmosis.tokenfactory.v1beta1.MsgMint".to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(74_981_956_874_579_206461u128).to_string(),
                        }),

                        mint_to_address: String::from("addr0000"),
                        sender: env.contract.address.to_string(),
                    }
                    .encode_to_vec()
                )
            },
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    // Check wrong argument
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
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
    assert_eq!(res.to_string(), "Generic error: Native token balance mismatch between the argument (50000000000000000000uusd) and the transferred (100000000000000000000uusd)");

    // Initialize token balances with a ratio of 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 100_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(
                &String::from(MOCK_CONTRACT_ADDR),
                &Uint128::new(100_000000000000000000),
            )],
        ),
        (
            &String::from("asset0000"),
            &[(
                &String::from(MOCK_CONTRACT_ADDR),
                &Uint128::new(100_000000000000000000),
            )],
        ),
    ]);

    // Initialize token balances with a ratio of 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 98_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Initialize token balances with a ratio of 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 100_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Successfully provide liquidity
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                amount: Uint128::from(99_000000000000000000u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(100_000000000000000000u128),
            },
        ],
        slippage_tolerance: Some(Decimal::percent(1)),
        auto_stake: None,
        receiver: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000000000000000000u128),
        }],
    );
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // Initialize token balances with a ratio of 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 99_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Successfully provide liquidity
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
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
                amount: Uint128::from(99_000000000000000000u128),
            },
        ],
        slippage_tolerance: Some(Decimal::percent(1)),
        auto_stake: None,
        receiver: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(99_000000000000000000u128),
        }],
    );
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();
}

#[test]
fn withdraw_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(100u128),
    }]);

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
            })
            .unwrap(),
        ),
    };

    let denom = format!("factory/{}/{}", env.contract.address, "share/astroport");

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(100u128))],
    )]);

    deps.querier.with_balance(&[(
        &String::from("asset0000"),
        &[Coin {
            denom: denom.to_string(),
            amount: Uint128::new(100u128),
        }],
    )]);

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

    // Withdraw liquidity
    let msg = ExecuteMsg::WithdrawLiquidity { assets: vec![] };

    let env = mock_env();
    let info = mock_info("addr0000", &[coin(100u128, denom.clone())]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
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
                    amount: Uint128::from(100u128),
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
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
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
        msg_burn_liquidity,
        &SubMsg {
            msg: CosmosMsg::Stargate {
                type_url: "/osmosis.tokenfactory.v1beta1.MsgBurn".to_string(),
                value: Binary::from(
                    MsgBurn {
                        sender: env.contract.address.to_string(),
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(100u128).to_string(),
                        }),
                        burn_from_address: "addr0000".to_string()
                    }
                    .encode_to_vec()
                ),
            },
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
        &attr("refund_assets", "100uusd, 100asset0000")
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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
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
        ask_asset_info: None,
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
        None,
    )
    .unwrap();
    assert!(expected_return_amount.abs_diff(simulation_res.return_amount) <= Uint128::one());
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert!(expected_spread_amount.abs_diff(simulation_res.spread_amount) <= Uint128::one());

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "uusd"),
            attr("ask_asset", "asset0000"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", 1487928894.to_string()),
            attr("spread_amount", 7593888.to_string()),
            attr("commission_amount", expected_commission_amount.to_string()),
            attr("maker_fee_amount", expected_maker_fee_amount.to_string()),
            attr("fee_share_amount", "0"),
        ]
    );

    assert_eq!(
        &SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: String::from("asset0000"),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: String::from("addr0000"),
                    amount: Uint128::from(1487928894u128),
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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
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
        ask_asset_info: None,
        belief_price: None,
        max_spread: None,
        to: None,
    };
    let env = mock_env_with_block_time(1000);
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Cw20DirectSwap {});

    // Normal sell
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_json_binary(&Cw20HookMsg::Swap {
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

    // Check simulation result
    // Return asset token balance as normal
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
        None,
    )
    .unwrap();
    assert!(expected_return_amount.abs_diff(simulation_res.return_amount) <= Uint128::one());
    assert_eq!(expected_commission_amount, simulation_res.commission_amount);
    assert!(expected_spread_amount.abs_diff(simulation_res.spread_amount) <= Uint128::one());

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "swap"),
            attr("sender", "addr0000"),
            attr("receiver", "addr0000"),
            attr("offer_asset", "asset0000"),
            attr("ask_asset", "uusd"),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", 1500851252.to_string()),
            attr("spread_amount", expected_spread_amount.to_string()),
            attr("commission_amount", expected_commission_amount.to_string()),
            attr("maker_fee_amount", expected_maker_fee_amount.to_string()),
            attr("fee_share_amount", "0"),
        ]
    );

    assert_eq!(
        &SubMsg {
            msg: CosmosMsg::Bank(BankMsg::Send {
                to_address: String::from("addr0000"),
                amount: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: 1500851252u128.into(),
                }],
            })
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        },
        msg_transfer,
    );

    // Failed due to non asset token contract being used in a swap
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: String::from("addr0000"),
        amount: offer_amount,
        msg: to_json_binary(&Cw20HookMsg::Swap {
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

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    let denom = format!("factory/{}/{}", env.contract.address, "share/astroport");

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
    )]);

    deps.querier.with_balance(&[(
        &"addr0000".to_string(),
        &[coin(total_share_amount.u128(), denom.clone())],
    )]);

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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
            })
            .unwrap(),
        ),
    };

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

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

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: asset_0_amount,
    }]);

    let denom = format!("factory/{}/{}", env.contract.address, "share/astroport");

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(&String::from(MOCK_CONTRACT_ADDR), &asset_1_amount)],
    )]);

    deps.querier.with_balance(&[(
        &"addr0000".to_string(),
        &[coin(total_share_amount.u128(), denom.clone())],
    )]);

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
        factory_addr: String::from("factory"),
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: None,
            })
            .unwrap(),
        ),
    };

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store the liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

    let res = query_share(deps.as_ref(), Uint128::new(250)).unwrap();

    assert_eq!(res[0].amount, Uint128::new(125));
    assert_eq!(res[1].amount, Uint128::new(500));
}

pub fn f64_to_dec<T>(val: f64) -> T
where
    T: FromStr,
    T::Err: Error,
{
    T::from_str(&val.to_string()).unwrap()
}

#[test]
fn observations_full_buffer() {
    let mut deps = mock_dependencies(&[]);
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(100_000);
    BufferManager::init(&mut deps.storage, OBSERVATIONS, 20).unwrap();

    let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

    let err = query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 11000).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Buffer is empty");

    let array = (1..=30)
        .into_iter()
        .map(|i| Observation {
            ts: env.block.time.seconds() + i * 1000,
            price: Default::default(),
            price_sma: Decimal::from_ratio(i, i * i),
        })
        .collect_vec();
    buffer.push_many(&array);
    buffer.commit(&mut deps.storage).unwrap();

    env.block.time = env.block.time.plus_seconds(30_000);

    assert_eq!(
        OracleObservation {
            timestamp: 120_000,
            price: f64_to_dec(20.0 / 400.0),
        },
        query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 10000).unwrap()
    );

    assert_eq!(
        OracleObservation {
            timestamp: 124_411,
            price: f64_to_dec(0.04098166666666694),
        },
        query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 5589).unwrap()
    );

    let err = query_observation(deps.as_ref(), env, OBSERVATIONS, 35_000).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Requested observation is too old. Last known observation is at 111000"
    );
}

#[test]
fn observations_incomplete_buffer() {
    let mut deps = mock_dependencies(&[]);
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(100_000);
    BufferManager::init(&mut deps.storage, OBSERVATIONS, 3000).unwrap();

    let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

    let err = query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 11000).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Buffer is empty");

    let array = (1..=30)
        .into_iter()
        .map(|i| Observation {
            ts: env.block.time.seconds() + i * 1000,
            price: Default::default(),
            price_sma: Decimal::from_ratio(i, i * i),
        })
        .collect_vec();
    buffer.push_many(&array);
    buffer.commit(&mut deps.storage).unwrap();

    env.block.time = env.block.time.plus_seconds(30_000);

    assert_eq!(
        OracleObservation {
            timestamp: 120_000,
            price: f64_to_dec(20.0 / 400.0),
        },
        query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 10000).unwrap()
    );

    assert_eq!(
        OracleObservation {
            timestamp: 124_411,
            price: f64_to_dec(0.04098166666666694),
        },
        query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 5589).unwrap()
    );
}

#[test]
fn observations_checking_triple_capacity_step_by_step() {
    let mut deps = mock_dependencies(&[]);
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(100_000);
    const CAPACITY: u32 = 20;
    BufferManager::init(&mut deps.storage, OBSERVATIONS, CAPACITY).unwrap();

    let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

    let ts = env.block.time.seconds();

    let array = (1..=CAPACITY * 3)
        .into_iter()
        .map(|i| Observation {
            ts: ts + i as u64 * 1000,
            price: Default::default(),
            price_sma: Decimal::from_ratio(i * i, i),
        })
        .collect_vec();

    for (k, obs) in array.iter().enumerate() {
        env.block.time = env.block.time.plus_seconds(1000);

        buffer.push(&obs);
        buffer.commit(&mut deps.storage).unwrap();
        let k1 = k as u32 + 1;

        let from = k1.saturating_sub(CAPACITY) + 1;
        let to = k1;

        for i in from..=to {
            let shift = (to - i) as u64;
            if shift != 0 {
                assert_eq!(
                    OracleObservation {
                        timestamp: ts + i as u64 * 1000 + 500,
                        price: f64_to_dec(i as f64 + 0.5),
                    },
                    query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, shift * 1000 - 500)
                        .unwrap()
                );
            }
            assert_eq!(
                OracleObservation {
                    timestamp: ts + i as u64 * 1000,
                    price: f64_to_dec(i as f64),
                },
                query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, shift * 1000).unwrap()
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

proptest! {
    #[test]
    fn constant_product_swap_no_fee(
        balance_in in 1000..1_000_000_000_000_000_000u128,
        balance_out in 1000..1_000_000_000_000_000_000u128,
        amount_in in 1000..100_000_000_000u128,
        amp in 1..150u64
    ) {
        prop_assume!(amount_in < balance_in && balance_out > balance_in);

        let offer_asset = native_asset("uusd".to_string(), Uint128::from(amount_in));
        let ask_asset = native_asset_info("uluna".to_string());

        let msg = InstantiateMsg {
            factory_addr: String::from("factory"),
            asset_infos: vec![offer_asset.info.clone(), ask_asset.clone()],
            token_code_id: 10u64,
            init_params: Some(to_json_binary(&StablePoolParams { amp, owner: None }).unwrap()),
        };

        let env = mock_env();
        let info = mock_info("owner", &[]);
        let mut deps = mock_dependencies(&[coin(balance_in, "uusd"), coin(balance_out, "uluna")]);

        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        let pools = config
            .pair_info
            .query_pools_decimal(&deps.as_ref().querier, &env.contract.address, &config.factory_addr)
            .unwrap();
        let (offer_pool, ask_pool) =
        select_pools(Some(&offer_asset.info), None, &pools).unwrap();

        let result = compute_swap(
            deps.as_ref().storage,
            &env,
            &config,
            &offer_asset.to_decimal_asset(offer_asset.info.decimals(&deps.as_ref().querier, &config.factory_addr).unwrap()).unwrap(),
            &offer_pool,
            &ask_pool,
            &pools,
        )
        .unwrap();

        let model: StableSwapModel = StableSwapModel::new(amp.into(), vec![balance_in, balance_out], 2);
        let sim_result = model.sim_exchange(0, 1, amount_in);

        let diff = (sim_result as i128 - result.return_amount.u128() as i128).abs();

        assert!(
            diff <= 20,
            "result={}, sim_result={}, amp={}, amount_in={}, balance_in={}, balance_out={}, diff={}",
            result.return_amount,
            sim_result,
            amp,
            amount_in,
            balance_in,
            balance_out,
            diff
        );

        let reverse_result = query_reverse_simulation(
            deps.as_ref(),
            env.clone(),
            native_asset("uluna".to_string(), result.return_amount),
            None,
        )
        .unwrap();

        let amount_in_f = amount_in as f64;
        let reverse_diff =
            (reverse_result.offer_amount.u128() as f64 - amount_in_f) / amount_in_f * 100.;

        assert!(
            reverse_diff <= 0.5,
            "result={}, sim_result={}, amp={}, amount_out={}, balance_in={}, balance_out={}, diff(%)={}",
            reverse_result.offer_amount.u128(),
            amount_in,
            amp,
            result.return_amount.u128(),
            balance_in,
            balance_out,
            reverse_diff
        );
    }
}

#[test]
fn update_owner() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let msg = InstantiateMsg {
        asset_infos: vec![
            AssetInfo::NativeToken {
                denom: "ucosmos".to_string(),
            },
            AssetInfo::NativeToken {
                denom: "ustake".to_string(),
            },
        ],
        factory_addr: "factory".to_owned(),
        token_code_id: 123u64,
        init_params: Some(
            to_json_binary(&StablePoolParams {
                amp: 100,
                owner: Some(owner.to_owned()),
            })
            .unwrap(),
        ),
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let new_owner = String::from("new_owner");

    // New owner
    let env = mock_env();
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    let info = mock_info(new_owner.as_str(), &[]);

    // Unauthorized check
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let info = mock_info(new_owner.as_str(), &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();

    // Propose new owner
    let info = mock_info(owner, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(0, res.messages.len());

    // Unauthorized ownership claim
    let info = mock_info("invalid_addr", &[]);
    let err = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Drop new owner
    let info = mock_info(owner, &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::DropOwnershipProposal {},
    )
    .unwrap();

    // Claim ownership
    let info = mock_info(new_owner.as_str(), &[]);
    let err = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    let info = mock_info(owner, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Claim ownership
    let info = mock_info(new_owner.as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // Let's query the state
    let config: ConfigResponse =
        from_json(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(new_owner, config.owner);
}
