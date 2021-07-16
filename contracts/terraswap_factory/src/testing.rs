use cosmwasm_std::{attr, from_binary, to_binary, Addr, CanonicalAddr, CosmosMsg, WasmMsg};

use crate::mock_querier::mock_dependencies;
use crate::{
    contract::{execute, instantiate, query},
    error::ContractError,
};

use crate::state::read_pair;

use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use terraswap::asset::{AssetInfo, PairInfo};
use terraswap::factory::{ConfigResponse, ExecuteMsg, InstantiateMsg, PairsResponse, QueryMsg};
use terraswap::hook::InitHook;
use terraswap::pair::InstantiateMsg as PairInstantiateMsg;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let pair_code_ids = vec![321u64, 455u64];

    let msg = InstantiateMsg {
        pair_code_ids: pair_code_ids.clone(),
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(pair_code_ids, config_res.pair_code_ids);
    assert_eq!(String::from("addr0000"), config_res.owner);
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_code_ids: vec![321u64],
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // update owner
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some("addr0001".to_string()),
        pair_code_ids: None,
        token_code_id: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(vec![321u64], config_res.pair_code_ids);
    assert_eq!(String::from("addr0001"), config_res.owner);

    // update left items
    let env = mock_env();
    let info = mock_info("addr0001", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: None,
        pair_code_ids: Some(vec![100u64, 321u64, 500u64]),
        token_code_id: Some(200u64),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!(vec![100u64, 321u64, 500u64], config_res.pair_code_ids);
    assert_eq!(String::from("addr0001"), config_res.owner);

    // Unauthorzied err
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: None,
        pair_code_ids: None,
        token_code_id: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {})
}

#[test]
fn create_pair() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_code_ids: vec![321u64],
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0001"),
        },
    ];

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // Check creating pair using non-whitelisted pair ID
    let msg = ExecuteMsg::CreatePair {
        pair_code_id: 100u64,
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    assert_eq!(res, ContractError::PairCodeNotAllowed {});

    let msg = ExecuteMsg::CreatePair {
        pair_code_id: 321u64,
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "create_pair"),
            attr("pair", "asset0000-asset0001")
        ]
    );
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            msg: to_binary(&PairInstantiateMsg {
                asset_infos: asset_infos.clone(),
                token_code_id: 123u64,
                init_hook: Some(InitHook {
                    contract_addr: String::from(MOCK_CONTRACT_ADDR),
                    msg: to_binary(&ExecuteMsg::Register {
                        asset_infos: asset_infos.clone()
                    })
                    .unwrap(),
                })
            })
            .unwrap(),
            code_id: 321u64,
            send: vec![],
            admin: None,
            label: String::new(),
        })]
    );

    let raw_infos = [
        asset_infos[0].to_raw(&deps.api).unwrap(),
        asset_infos[1].to_raw(&deps.api).unwrap(),
    ];
    let pair_info = read_pair(&deps.storage, &raw_infos).unwrap();

    assert_eq!(pair_info.contract_addr, CanonicalAddr::from(vec![]),);
}

#[test]
fn register() {
    let mut deps = mock_dependencies(&[]);
    let pair_code_id = 321u64;

    let msg = InstantiateMsg {
        pair_code_ids: vec![pair_code_id],
        token_code_id: 123u64,
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0001"),
        },
    ];

    let msg = ExecuteMsg::CreatePair {
        pair_code_id,
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();

    // register terraswap pair querier
    deps.querier.with_terraswap_pairs(&[(
        &String::from("pair0000"),
        &PairInfo {
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0000"),
            liquidity_token: Addr::unchecked("liquidity0000"),
        },
    )]);

    let msg = ExecuteMsg::Register {
        asset_infos: asset_infos.clone(),
    };

    let env = mock_env();
    let info = mock_info("pair0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let query_res = query(
        deps.as_ref(),
        env,
        QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        },
    )
    .unwrap();

    let pair_res: PairInfo = from_binary(&query_res).unwrap();
    assert_eq!(
        pair_res,
        PairInfo {
            liquidity_token: Addr::unchecked("liquidity0000"),
            contract_addr: Addr::unchecked("pair0000"),
            asset_infos: asset_infos.clone(),
        }
    );

    let msg = ExecuteMsg::Register {
        asset_infos: [asset_infos[1].clone(), asset_infos[0].clone()],
    };

    let env = mock_env();
    let info = mock_info("pair0000", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairWasRegistered {});

    // Store one more item to test query pairs
    let asset_infos_2 = [
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0002"),
        },
    ];

    let msg = ExecuteMsg::CreatePair {
        pair_code_id,
        asset_infos: asset_infos_2.clone(),
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();

    // register terraswap pair querier
    deps.querier.with_terraswap_pairs(&[(
        &String::from("pair0001"),
        &PairInfo {
            asset_infos: [
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0001"),
            liquidity_token: Addr::unchecked("liquidity0001"),
        },
    )]);

    let msg = ExecuteMsg::Register {
        asset_infos: asset_infos_2.clone(),
    };

    let env = mock_env();
    let info = mock_info("pair0001", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let query_msg = QueryMsg::Pairs {
        start_after: None,
        limit: None,
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![
            PairInfo {
                liquidity_token: Addr::unchecked("liquidity0000"),
                contract_addr: Addr::unchecked("pair0000"),
                asset_infos: asset_infos.clone(),
            },
            PairInfo {
                liquidity_token: Addr::unchecked("liquidity0001"),
                contract_addr: Addr::unchecked("pair0001"),
                asset_infos: asset_infos_2.clone(),
            }
        ]
    );

    let query_msg = QueryMsg::Pairs {
        start_after: None,
        limit: Some(1),
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: Addr::unchecked("liquidity0000"),
            contract_addr: Addr::unchecked("pair0000"),
            asset_infos: asset_infos.clone(),
        }]
    );

    let query_msg = QueryMsg::Pairs {
        start_after: Some(asset_infos.clone()),
        limit: None,
    };

    let res = query(deps.as_ref(), env, query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: Addr::unchecked("liquidity0001"),
            contract_addr: Addr::unchecked("pair0001"),
            asset_infos: asset_infos_2.clone(),
        }]
    );
}
