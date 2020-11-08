use cosmwasm_std::{
    from_binary, log, to_binary, CanonicalAddr, CosmosMsg, HumanAddr, StdError, WasmMsg,
};

use crate::contract::{handle, init, query};
use crate::mock_querier::mock_dependencies;
use crate::msg::{ConfigResponse, HandleMsg, InitMsg, PairsResponse, QueryMsg};
use crate::state::read_pair;

use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use terraswap::{AssetInfo, InitHook, PairInfo, PairInitMsg};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        pair_code_id: 321u64,
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    let query_res = query(&deps, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(321u64, config_res.pair_code_id);
    assert_eq!(HumanAddr::from("addr0000"), config_res.owner);
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        pair_code_id: 321u64,
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    // update owner
    let env = mock_env("addr0000", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr("addr0001".to_string())),
        pair_code_id: None,
        token_code_id: None,
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(&deps, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(321u64, config_res.pair_code_id);
    assert_eq!(HumanAddr::from("addr0001"), config_res.owner);

    // update left items
    let env = mock_env("addr0001", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: None,
        pair_code_id: Some(100u64),
        token_code_id: Some(200u64),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(&deps, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!(100u64, config_res.pair_code_id);
    assert_eq!(HumanAddr::from("addr0001"), config_res.owner);

    // Unauthorzied err
    let env = mock_env("addr0000", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: None,
        pair_code_id: None,
        token_code_id: None,
    };

    let res = handle(&mut deps, env, msg);
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn create_pair() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        pair_code_id: 321u64,
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = init(&mut deps, env, msg).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0001"),
        },
    ];

    let msg = HandleMsg::CreatePair {
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.log,
        vec![
            log("action", "create_pair"),
            log("pair", "asset0000-asset0001")
        ]
    );
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            msg: to_binary(&PairInitMsg {
                asset_infos: asset_infos.clone(),
                token_code_id: 123u64,
                init_hook: Some(InitHook {
                    contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                    msg: to_binary(&HandleMsg::Register {
                        asset_infos: asset_infos.clone()
                    })
                    .unwrap(),
                })
            })
            .unwrap(),
            code_id: 321u64,
            send: vec![],
            label: None,
        })]
    );

    let raw_infos = [
        asset_infos[0].to_raw(&deps).unwrap(),
        asset_infos[1].to_raw(&deps).unwrap(),
    ];
    let pair_info = read_pair(&deps.storage, &raw_infos).unwrap();

    assert_eq!(pair_info.contract_addr, CanonicalAddr::default(),);
}

#[test]
fn register() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        pair_code_id: 321u64,
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);
    let _res = init(&mut deps, env, msg).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0001"),
        },
    ];

    let msg = HandleMsg::CreatePair {
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);
    let _res = handle(&mut deps, env, msg).unwrap();

    // register terraswap pair querier
    deps.querier.with_terraswap_pairs(&[(
        &HumanAddr::from("pair0000"),
        &PairInfo {
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: HumanAddr::from("pair0000"),
            liquidity_token: HumanAddr::from("liquidity0000"),
        },
    )]);

    let msg = HandleMsg::Register {
        asset_infos: asset_infos.clone(),
    };

    let env = mock_env("pair0000", &[]);
    let _res = handle(&mut deps, env, msg).unwrap();

    let query_res = query(
        &deps,
        QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        },
    )
    .unwrap();

    let pair_res: PairInfo = from_binary(&query_res).unwrap();
    assert_eq!(
        pair_res,
        PairInfo {
            liquidity_token: HumanAddr::from("liquidity0000"),
            contract_addr: HumanAddr::from("pair0000"),
            asset_infos: asset_infos.clone(),
        }
    );

    let msg = HandleMsg::Register {
        asset_infos: [asset_infos[1].clone(), asset_infos[0].clone()],
    };

    let env = mock_env("pair0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap_err();
    match res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Pair was already registered"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    // Store one more item to test query pairs
    let asset_infos_2 = [
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: HumanAddr::from("asset0002"),
        },
    ];

    let msg = HandleMsg::CreatePair {
        asset_infos: asset_infos_2.clone(),
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);
    let _res = handle(&mut deps, env, msg).unwrap();

    // register terraswap pair querier
    deps.querier.with_terraswap_pairs(&[(
        &HumanAddr::from("pair0001"),
        &PairInfo {
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: HumanAddr::from("pair0001"),
            liquidity_token: HumanAddr::from("liquidity0001"),
        },
    )]);

    let msg = HandleMsg::Register {
        asset_infos: asset_infos_2.clone(),
    };

    let env = mock_env("pair0001", &[]);
    let _res = handle(&mut deps, env, msg).unwrap();

    let query_msg = QueryMsg::Pairs {
        start_after: None,
        limit: None,
    };

    let res = query(&mut deps, query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![
            PairInfo {
                liquidity_token: HumanAddr::from("liquidity0000"),
                contract_addr: HumanAddr::from("pair0000"),
                asset_infos: asset_infos.clone(),
            },
            PairInfo {
                liquidity_token: HumanAddr::from("liquidity0001"),
                contract_addr: HumanAddr::from("pair0001"),
                asset_infos: asset_infos_2.clone(),
            }
        ]
    );

    let query_msg = QueryMsg::Pairs {
        start_after: None,
        limit: Some(1),
    };

    let res = query(&mut deps, query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: HumanAddr::from("liquidity0000"),
            contract_addr: HumanAddr::from("pair0000"),
            asset_infos: asset_infos.clone(),
        }]
    );

    let query_msg = QueryMsg::Pairs {
        start_after: Some(asset_infos.clone()),
        limit: None,
    };

    let res = query(&mut deps, query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: HumanAddr::from("liquidity0001"),
            contract_addr: HumanAddr::from("pair0001"),
            asset_infos: asset_infos_2.clone(),
        }]
    );
}
