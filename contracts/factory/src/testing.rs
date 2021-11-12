use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, ContractResult, Reply, ReplyOn, SubMsg,
    SubMsgExecutionResponse, WasmMsg,
};

use crate::mock_querier::mock_dependencies;
use crate::state::CONFIG;
use crate::{
    contract::{execute, instantiate, query},
    error::ContractError,
};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, FeeInfoResponse, InstantiateMsg, PairConfig, PairType,
    PairsResponse, QueryMsg,
};

use crate::contract::reply;
use crate::response::MsgInstantiateContractResponse;
use astroport::pair::InstantiateMsg as PairInstantiateMsg;
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use protobuf::Message;

#[test]
fn pair_type_to_string() {
    assert_eq!(PairType::Xyk {}.to_string(), "xyk");
    assert_eq!(PairType::Stable {}.to_string(), "stable");
}

#[test]
fn proper_initialization() {
    // check validation of total and maker fee bps
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        pair_xyk_config: None,
        pair_stable_config: Some(PairConfig {
            code_id: 325u64,
            total_fee_bps: 10_001,
            maker_fee_bps: 10,
        }),
        token_code_id: 123u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        generator_address: Addr::unchecked("generator"),
        owner: owner.clone(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairConfigInvalidFeeBps {});

    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_xyk_config: Some(PairConfig {
            code_id: 123u64,
            total_fee_bps: 100,
            maker_fee_bps: 10,
        }),
        pair_stable_config: Some(PairConfig {
            code_id: 325u64,
            total_fee_bps: 100,
            maker_fee_bps: 10,
        }),
        token_code_id: 123u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        generator_address: Addr::unchecked("generator"),
        owner: owner.clone(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(msg.pair_xyk_config, config_res.pair_xyk_config);
    assert_eq!(msg.pair_stable_config, config_res.pair_stable_config);
    assert_eq!(Addr::unchecked(owner), config_res.owner);
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let pair_config = PairConfig {
        code_id: 123u64,
        total_fee_bps: 3,
        maker_fee_bps: 166,
    };

    let msg = InstantiateMsg {
        pair_xyk_config: Some(pair_config.clone()),
        pair_stable_config: None,
        token_code_id: 123u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        owner: owner.to_string(),
        generator_address: Addr::unchecked("generator"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let new_owner = "addr0001";

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // update owner
    let env = mock_env();
    let info = mock_info(owner.clone(), &[]);
    let msg = ExecuteMsg::UpdateConfig {
        gov: None,
        owner: Some(Addr::unchecked(new_owner)),
        token_code_id: None,
        fee_address: Some(Addr::unchecked("fee_addr")),
        generator_address: None,
        pair_xyk_config: Some(pair_config.clone()),
        pair_stable_config: Some(pair_config.clone()),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(Some(pair_config.clone()), config_res.pair_xyk_config);
    assert_eq!(Some(pair_config.clone()), config_res.pair_stable_config);
    assert_eq!(Addr::unchecked(new_owner), config_res.owner);

    // check validation of total and maker fee bps
    let pair_config = PairConfig {
        code_id: 123u64,
        total_fee_bps: 3,
        maker_fee_bps: 10_001,
    };

    let env = mock_env();
    let info = mock_info(new_owner, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        gov: None,
        owner: Some(Addr::unchecked(new_owner)),
        token_code_id: None,
        fee_address: Some(Addr::unchecked("fee_addr")),
        generator_address: None,
        pair_xyk_config: Some(pair_config),
        pair_stable_config: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairConfigInvalidFeeBps {});

    // update left items
    let env = mock_env();
    let info = mock_info(new_owner, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        gov: None,
        owner: None,
        token_code_id: Some(200u64),
        fee_address: None,
        generator_address: None,
        pair_xyk_config: None,
        pair_stable_config: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!(String::from("addr0001"), config_res.owner);

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        gov: None,
        owner: None,
        token_code_id: None,
        fee_address: None,
        generator_address: None,
        pair_xyk_config: None,
        pair_stable_config: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Check fee response
    let pair_config = PairConfig {
        code_id: 123u64,
        total_fee_bps: 3,
        maker_fee_bps: 166,
    };

    let env = mock_env();
    let info = mock_info(new_owner, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        gov: None,
        owner: Some(Addr::unchecked(new_owner)),
        token_code_id: None,
        fee_address: Some(Addr::unchecked("fee_addr")),
        generator_address: None,
        pair_xyk_config: Some(pair_config.clone()),
        pair_stable_config: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let query_res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::FeeInfo {
            pair_type: PairType::Xyk {},
        },
    )
    .unwrap();
    let fee_info: FeeInfoResponse = from_binary(&query_res).unwrap();
    assert_eq!(String::from("fee_addr"), fee_info.fee_address.unwrap());
    assert_eq!(pair_config.total_fee_bps, fee_info.total_fee_bps);
    assert_eq!(pair_config.maker_fee_bps, fee_info.maker_fee_bps);
}

#[test]
fn create_pair() {
    let mut deps = mock_dependencies(&[]);

    let pair_config = PairConfig {
        code_id: 321u64,
        total_fee_bps: 100,
        maker_fee_bps: 10,
    };

    let msg = InstantiateMsg {
        pair_xyk_config: Some(pair_config.clone()),
        pair_stable_config: None,
        token_code_id: 123u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        owner: "owner0000".to_string(),
        generator_address: Addr::unchecked("generator"),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg.clone()).unwrap();

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0000"),
        },
        AssetInfo::Token {
            contract_addr: Addr::unchecked("asset0001"),
        },
    ];

    let config = CONFIG.load(&deps.storage);
    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // Check creating pair using non-whitelisted pair ID
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::CreatePairStable {
            asset_infos: asset_infos.clone(),
            amp: 100,
            init_hook: None,
        },
    )
    .unwrap_err();
    assert_eq!(res, ContractError::PairConfigNotFound {});

    let res = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::CreatePair {
            asset_infos: asset_infos.clone(),
            init_hook: None,
        },
    )
    .unwrap();

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "create_pair"),
            attr("pair", "asset0000-asset0001")
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Instantiate {
                msg: to_binary(&PairInstantiateMsg {
                    factory_addr: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    asset_infos: asset_infos.clone(),
                    token_code_id: msg.token_code_id,
                })
                .unwrap(),
                code_id: pair_config.code_id,
                funds: vec![],
                admin: Some(config.unwrap().owner.to_string()),
                label: String::from("Astroport pair"),
            }
            .into(),
            id: 1,
            gas_limit: None,
            reply_on: ReplyOn::Success
        }]
    );
}

#[test]
fn register() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let msg = InstantiateMsg {
        pair_xyk_config: Some(PairConfig {
            code_id: 123u64,
            total_fee_bps: 100,
            maker_fee_bps: 10,
        }),
        pair_stable_config: None,
        token_code_id: 123u64,
        init_hook: None,
        fee_address: None,
        gov: None,
        generator_address: Addr::unchecked("generator"),
        owner: owner.to_string(),
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
        asset_infos: asset_infos.clone(),
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let pair0_addr = "pair0000".to_string();
    let pair0_info = PairInfo {
        asset_infos: asset_infos.clone(),
        contract_addr: Addr::unchecked("pair0000"),
        liquidity_token: Addr::unchecked("liquidity0000"),
        pair_type: PairType::Xyk {},
    };

    let mut deployed_pairs = vec![(&pair0_addr, &pair0_info)];

    // register terraswap pair querier
    deps.querier.with_astroport_pairs(&deployed_pairs);

    let data = MsgInstantiateContractResponse {
        contract_address: String::from("pair0000"),
        data: vec![],
        unknown_fields: Default::default(),
        cached_size: Default::default(),
    }
    .write_to_bytes()
    .unwrap();

    let reply_msg = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(data.into()),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg.clone()).unwrap();

    let query_res = query(
        deps.as_ref(),
        env.clone(),
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
            pair_type: PairType::Xyk {},
        }
    );

    // check pair was registered
    let res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap_err();
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
        asset_infos: asset_infos_2.clone(),
        init_hook: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let pair1_addr = "pair0001".to_string();
    let pair1_info = PairInfo {
        asset_infos: asset_infos_2.clone(),
        contract_addr: Addr::unchecked("pair0001"),
        liquidity_token: Addr::unchecked("liquidity0001"),
        pair_type: PairType::Xyk {},
    };

    deployed_pairs.push((&pair1_addr, &pair1_info));

    // register terraswap pair querier
    deps.querier.with_astroport_pairs(&deployed_pairs);

    let data = MsgInstantiateContractResponse {
        contract_address: String::from("pair0001"),
        data: vec![],
        unknown_fields: Default::default(),
        cached_size: Default::default(),
    }
    .write_to_bytes()
    .unwrap();

    let reply_msg_2 = Reply {
        id: 1,
        result: ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(data.into()),
        }),
    };

    let _res = reply(deps.as_mut(), mock_env(), reply_msg_2.clone()).unwrap();

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
                pair_type: PairType::Xyk {},
            },
            PairInfo {
                liquidity_token: Addr::unchecked("liquidity0001"),
                contract_addr: Addr::unchecked("pair0001"),
                asset_infos: asset_infos_2.clone(),
                pair_type: PairType::Xyk {},
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
            pair_type: PairType::Xyk {},
        }]
    );

    let query_msg = QueryMsg::Pairs {
        start_after: Some(asset_infos.clone()),
        limit: None,
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: Addr::unchecked("liquidity0001"),
            contract_addr: Addr::unchecked("pair0001"),
            asset_infos: asset_infos_2.clone(),
            pair_type: PairType::Xyk {},
        }]
    );

    // Deregister from wrong acc
    let env = mock_env();
    let info = mock_info("wrong_addr0000", &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::Deregister {
            asset_infos: asset_infos_2.clone(),
        },
    )
    .unwrap_err();

    assert_eq!(res, ContractError::Unauthorized {});

    // Proper deregister
    let env = mock_env();
    let info = mock_info(owner.clone(), &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::Deregister {
            asset_infos: asset_infos_2.clone(),
        },
    )
    .unwrap();

    assert_eq!(res.attributes[0], attr("action", "deregister"));

    let query_msg = QueryMsg::Pairs {
        start_after: None,
        limit: None,
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PairsResponse = from_binary(&res).unwrap();
    assert_eq!(
        pairs_res.pairs,
        vec![PairInfo {
            liquidity_token: Addr::unchecked("liquidity0000"),
            contract_addr: Addr::unchecked("pair0000"),
            asset_infos: asset_infos.clone(),
            pair_type: PairType::Xyk {},
        },]
    );
}
