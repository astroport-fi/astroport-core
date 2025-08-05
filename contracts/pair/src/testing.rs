use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, coin, to_json_binary, Addr, BankMsg, Binary, BlockInfo, Coin, CosmosMsg, Decimal,
    DepsMut, Env, Reply, ReplyOn, Response, StdError, SubMsg, SubMsgResponse, SubMsgResult,
    Timestamp, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use proptest::prelude::*;
use prost::Message;

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::common::LP_SUBDENOM;
use astroport::factory::PairType;
use astroport::pair::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, PoolResponse, ReverseSimulationResponse,
    SimulationResponse, TWAP_PRECISION,
};
use astroport::token_factory::{MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint};

use crate::contract::compute_offer_amount;
use crate::contract::reply;
use crate::contract::{
    accumulate_prices, assert_max_spread, compute_swap, execute, instantiate, query_pool,
    query_reverse_simulation, query_share, query_simulation,
};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::{Config, CONFIG};

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
        pair_type: PairType::Xyk {},
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
        init_params: None,
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
                type_url: MsgCreateDenom::TYPE_URL.to_string(),
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
    ]);

    let msg = InstantiateMsg {
        pair_type: PairType::Xyk {},
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
        init_params: None,
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
        min_lp_to_receive: None,
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
                type_url: MsgMint::TYPE_URL.to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(1000_u128).to_string(),
                        }),
                        sender: env.contract.address.to_string(),
                        #[cfg(not(feature = "injective"))]
                        mint_to_address: String::from(MOCK_CONTRACT_ADDR),
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
                type_url: MsgMint::TYPE_URL.to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(99_999999999999999000u128).to_string(),
                        }),
                        sender: env.contract.address.to_string(),
                        #[cfg(not(feature = "injective"))]
                        mint_to_address: String::from("addr0000"),
                    }
                    .encode_to_vec()
                )
            },
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }
    );

    // Provide more liquidity 1:2, which is not propotional to 1:1,
    // It must accept 1:1 and treat the leftover amount as a donation
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(200_000000000000000000 + 200_000000000000000000 /* user deposit must be pre-applied */),
        }],
    ),   (
        &String::from("liquidity0000"),
        &[Coin {
            denom: denom.to_string(),
            amount: Uint128::new(100_000000000000000000),
        }],
    )]);

    deps.querier.with_token_balances(&[(
        &String::from("asset0000"),
        &[(
            &String::from(MOCK_CONTRACT_ADDR),
            &Uint128::new(200_000000000000000000),
        )],
    )]);

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
        slippage_tolerance: Some(Decimal::percent(50)),
        auto_stake: None,
        receiver: None,
        min_lp_to_receive: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(200_000000000000000000u128),
        }],
    );

    // Only accept 100, then 50 share will be generated with 100 * (100 / 200)
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
                type_url: MsgMint::TYPE_URL.to_string(),
                value: Binary::from(
                    MsgMint {
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(50_000000000000000000u128).to_string(),
                        }),
                        sender: env.contract.address.to_string(),
                        #[cfg(not(feature = "injective"))]
                        mint_to_address: String::from("addr0000"),
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
        min_lp_to_receive: None,
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
    assert_eq!(
        res,
        ContractError::Std(StdError::generic_err(
            "Native token balance mismatch between the argument (50000000000000000000uusd) and the transferred (100000000000000000000uusd)",
        ))
    );

    // Initialize token amount to the 1:1 ratio
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

    // Failed because the price is under slippage_tolerance
    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                amount: Uint128::from(98_000000000000000000u128),
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
        min_lp_to_receive: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000000000000000000u128),
        }],
    );
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::MaxSlippageAssertion {});

    // Initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 98_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Failed because the price is under slippage_tolerance
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
                amount: Uint128::from(98_000000000000000000u128),
            },
        ],
        slippage_tolerance: Some(Decimal::percent(1)),
        auto_stake: None,
        receiver: None,
        min_lp_to_receive: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(98_000000000000000000u128),
        }],
    );
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::MaxSlippageAssertion {});

    // Initialize token amount with a 1:1 ratio
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 100_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Successfully provides liquidity
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
        min_lp_to_receive: None,
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

    // Initialize token balance to 1:1
    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100_000000000000000000 + 99_000000000000000000 /* user deposit must be pre-applied */),
        }],
    )]);

    // Successfully provides liquidity
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
        min_lp_to_receive: None,
    };

    let env = mock_env_with_block_time(env.block.time.seconds() + 1000);
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(99_000000000000000000u128),
        }],
    );
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                amount: Uint128::zero(),
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
        min_lp_to_receive: None,
    };
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(99_000000000000000000u128),
        }],
    );
    let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidZeroAmount {});

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
        slippage_tolerance: Some(Decimal::percent(51)),
        auto_stake: None,
        receiver: None,
        min_lp_to_receive: None,
    };
    let info = mock_info(
        "addr0001",
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000000000000000000u128),
        }],
    );
    let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(err, ContractError::AllowedSpreadAssertion {});
}

#[test]
fn withdraw_liquidity() {
    let mut deps = mock_dependencies(&[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::new(100u128),
    }]);

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    deps.querier.with_token_balances(&[
        (
            &String::from("liquidity0000"),
            &[(&String::from("addr0000"), &Uint128::new(100u128))],
        ),
        (
            &String::from("asset0000"),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(100u128))],
        ),
    ]);

    let msg = InstantiateMsg {
        pair_type: PairType::Xyk {},
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
        init_params: None,
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
    let msg = ExecuteMsg::WithdrawLiquidity {
        assets: vec![],
        min_assets_to_receive: None,
    };

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
                type_url: MsgBurn::TYPE_URL.to_string(),
                value: Binary::from(
                    MsgBurn {
                        sender: env.contract.address.to_string(),
                        amount: Some(astroport::token_factory::ProtoCoin {
                            denom: denom.to_string(),
                            amount: Uint128::from(100u128).to_string(),
                        }),
                        #[cfg(not(feature = "injective"))]
                        burn_from_address: "".to_string()
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
        pair_type: PairType::Xyk {},
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
        init_params: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store liquidity token
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

    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    // Current price is 1.5, so expected return without spread is 1000
    // 952380952 = 20000000000 - (30000000000 * 20000000000) / (30000000000 + 1500000000)
    let expected_ret_amount = Uint128::new(952_380_952u128);

    // 47619047 = 1500000000 * (20000000000 / 30000000000) - 952380952
    let expected_spread_amount = Uint128::new(47619047u128);

    let expected_commission_amount = expected_ret_amount.multiply_ratio(3u128, 1000u128); // 0.3%
    let expected_maker_fee_amount = expected_commission_amount.multiply_ratio(166u128, 1000u128); // 0.166

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

    let err = query_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            amount: offer_amount,
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Given offer asset does not belong in the pair"
    );

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
    let err = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::NativeToken {
                denom: "cny".to_string(),
            },
            amount: expected_return_amount,
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Given ask asset doesn't belong to pairs"
    );

    let reverse_simulation_res: ReverseSimulationResponse = query_reverse_simulation(
        deps.as_ref(),
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("asset0000"),
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
            attr("spread_amount", expected_spread_amount.to_string()),
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
    let total_share = Uint128::new(20000000000u128);
    let asset_pool_amount = Uint128::new(30000000000u128);
    let collateral_pool_amount = Uint128::new(20000000000u128);
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
        pair_type: PairType::Xyk {},
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
        init_params: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store liquidity token
    store_liquidity_token(deps.as_mut(), 1, "liquidity0000".to_string());

    // Unauthorized access; can not execute swap directly for token swap
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
            max_spread: Some(Decimal::percent(50)),
            to: None,
        })
        .unwrap(),
    });
    let env = mock_env_with_block_time(1000);
    let info = mock_info("asset0000", &[]);

    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg_transfer = res.messages.get(0).expect("no message");

    // Current price is 1.5, so expected return without spread is 1000
    // 952380952,3809524 = 20000000000 - (30000000000 * 20000000000) / (30000000000 + 1500000000)
    let expected_ret_amount = Uint128::new(952_380_952u128);

    // 47619047 = 1500000000 * (20000000000 / 30000000000) - 952380952,3809524
    let expected_spread_amount = Uint128::new(47619047u128);

    let expected_commission_amount = expected_ret_amount.multiply_ratio(3u128, 1000u128); // 0.3%
    let expected_maker_fee_amount = expected_commission_amount.multiply_ratio(166u128, 1000u128);
    let expected_return_amount = expected_ret_amount
        .checked_sub(expected_commission_amount)
        .unwrap();

    // Check simulation res
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
                    amount: expected_return_amount
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

    assert_max_spread(
        Some(Decimal::from_ratio(1200u128, 1u128)),
        Some(Decimal::percent(51)),
        Uint128::from(1200000000u128),
        Uint128::from(989999u128),
        Uint128::zero(),
    )
    .unwrap_err();
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
        pair_type: PairType::Xyk {},
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
        init_params: None,
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
        pair_type: PairType::Xyk {},
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
        init_params: None,
    };

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Store liquidity token
    store_liquidity_token(deps.as_mut(), 1, denom.to_string());

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
        price_x: u128,
        price_y: u128,
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
                x_amount: 250,
                y_amount: 500,
            },
            Result {
                block_time_last: 1000,
                price_x: 2000, // 500/250*1000
                price_y: 500,  // 250/500*1000
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
                x_amount: 250,
                y_amount: 500,
            },
            Result {
                block_time_last: 1000,
                price_x: 1,
                price_y: 2,
                is_some: false,
            },
        ),
        (
            Case {
                block_time: 1500,
                block_time_last: 1000,
                last0: 500 * price_precision,
                last1: 2000 * price_precision,
                x_amount: 250,
                y_amount: 500,
            },
            Result {
                block_time_last: 1500,
                price_x: 1500, // 500 + (500/250*500)
                price_y: 2250, // 2000 + (250/500*500)
                is_some: true,
            },
        ),
    ];

    for test_case in test_cases {
        let (case, result) = test_case;

        let env = mock_env_with_block_time(case.block_time);
        let config = accumulate_prices(
            env,
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
                    liquidity_token: "lp_token".to_owned(),
                    pair_type: PairType::Xyk {}, // Implemented in mock querier
                },
                factory_addr: Addr::unchecked("factory"),
                block_time_last: case.block_time_last,
                price0_cumulative_last: Uint128::new(case.last0),
                price1_cumulative_last: Uint128::new(case.last1),
                track_asset_balances: false,
                fee_share: None,
                tracker_addr: None,
            },
            Uint128::new(case.x_amount),
            Uint128::new(case.y_amount),
        )
        .unwrap();

        assert_eq!(result.is_some, config.is_some());

        if let Some(config) = config {
            assert_eq!(config.2, result.block_time_last);
            assert_eq!(
                config.0 / Uint128::from(price_precision),
                Uint128::new(result.price_x)
            );
            assert_eq!(
                config.1 / Uint128::from(price_precision),
                Uint128::new(result.price_y)
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
fn compute_swap_rounding() {
    let offer_pool = Uint128::from(5_000_000_000_000_u128);
    let ask_pool = Uint128::from(1_000_000_000_u128);
    let return_amount = Uint128::from(0_u128);
    let spread_amount = Uint128::from(0_u128);
    let commission_amount = Uint128::from(0_u128);
    let offer_amount = Uint128::from(1_u128);

    assert_eq!(
        compute_swap(offer_pool, ask_pool, offer_amount, Decimal::zero()),
        Ok((return_amount, spread_amount, commission_amount))
    );
}

proptest! {
    #[test]
    fn compute_swap_overflow_test(
        offer_pool in 1_000_000..9_000_000_000_000_000_000u128,
        ask_pool in 1_000_000..9_000_000_000_000_000_000u128,
        offer_amount in 1..100_000_000000u128,
    ) {

        let offer_pool = Uint128::from(offer_pool);
        let ask_pool = Uint128::from(ask_pool);
        let offer_amount = Uint128::from(offer_amount);
        let commission_amount = Decimal::zero();

        // Make sure there are no overflows
        compute_swap(
            offer_pool,
            ask_pool,
            offer_amount,
            commission_amount,
        ).unwrap();
    }
}

#[test]
fn ensure_useful_error_messages_are_given_on_swaps() {
    const OFFER: Uint128 = Uint128::new(1_000_000_000000);
    const ASK: Uint128 = Uint128::new(1_000_000_000000);
    const AMOUNT: Uint128 = Uint128::new(1_000000);
    const ZERO: Uint128 = Uint128::zero();
    const DZERO: Decimal = Decimal::zero();

    // Computing ask
    assert_eq!(
        compute_swap(ZERO, ZERO, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, ZERO, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, ASK, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(ZERO, ASK, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, ZERO, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, ZERO, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_swap(OFFER, ASK, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("Swap amount must not be zero")
    );
    compute_swap(OFFER, ASK, AMOUNT, DZERO).unwrap();

    // Computing offer
    assert_eq!(
        compute_offer_amount(ZERO, ZERO, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, ZERO, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, ASK, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(ZERO, ASK, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, ZERO, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, ZERO, AMOUNT, DZERO).unwrap_err(),
        StdError::generic_err("One of the pools is empty")
    );
    assert_eq!(
        compute_offer_amount(OFFER, ASK, ZERO, DZERO).unwrap_err(),
        StdError::generic_err("Swap amount must not be zero")
    );
    compute_offer_amount(OFFER, ASK, AMOUNT, DZERO).unwrap();
}
