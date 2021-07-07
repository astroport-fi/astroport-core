use std::time::{SystemTime, UNIX_EPOCH};

use cosmwasm_std::{
    from_binary, log, to_binary, CanonicalAddr, CosmosMsg, HumanAddr, StdError, Uint128, WasmMsg,
};

use crate::contract::{handle, init, query};
use crate::mock_querier::mock_dependencies;

use crate::state::read_pair;

use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use terraswap::asset::{AssetInfo, WeightedAssetInfo};
use terraswap::factory::{ConfigResponse, HandleMsg, InitMsg, QueryMsg};
use terraswap::hook::InitHook;
use terraswap::pair::InitMsg as PairInitMsg;

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
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let end_time = start_time + 1000;

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
        WeightedAssetInfo {
            info: AssetInfo::Token {
                contract_addr: HumanAddr::from("asset0000"),
            },
            start_weight: Uint128(30),
            end_weight: Uint128(20),
        },
        WeightedAssetInfo {
            info: AssetInfo::Token {
                contract_addr: HumanAddr::from("asset0001"),
            },
            start_weight: Uint128(30),
            end_weight: Uint128(20),
        },
    ];

    let msg = HandleMsg::CreatePair {
        asset_infos: asset_infos.clone(),
        start_time,
        end_time,
        init_hook: None,
        description: Some(String::from("description")),
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
                }),
                start_time,
                end_time,
                description: Some(String::from("description")),
            })
            .unwrap(),
            code_id: 321u64,
            send: vec![],
            label: None,
        })]
    );

    let raw_infos = [
        asset_infos[0].info.to_raw(&deps).unwrap(),
        asset_infos[1].info.to_raw(&deps).unwrap(),
    ];
    let pair_info = read_pair(&deps.storage, &raw_infos).unwrap();

    assert_eq!(pair_info.contract_addr, CanonicalAddr::default(),);
}
