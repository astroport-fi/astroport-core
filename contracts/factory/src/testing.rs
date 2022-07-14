use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Reply, ReplyOn, SubMsg, SubMsgResponse, SubMsgResult,
    WasmMsg,
};

use crate::mock_querier::mock_dependencies;
use crate::state::CONFIG;
use crate::{
    contract::{execute, instantiate, query},
    error::ContractError,
};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PairConfig, PairType, PairsResponse, QueryMsg,
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
    // Validate total and maker fee bps
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        pair_configs: vec![
            PairConfig {
                code_id: 123u64,
                pair_type: PairType::Xyk {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            },
            PairConfig {
                code_id: 325u64,
                pair_type: PairType::Xyk {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            },
        ],
        token_code_id: 123u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.clone(),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairConfigDuplicate {});

    let msg = InstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: 123u64,
            pair_type: PairType::Xyk {},
            total_fee_bps: 10_001,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id: 123u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.clone(),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairConfigInvalidFeeBps {});

    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_configs: vec![
            PairConfig {
                code_id: 325u64,
                pair_type: PairType::Stable {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            },
            PairConfig {
                code_id: 123u64,
                pair_type: PairType::Xyk {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            },
        ],
        token_code_id: 123u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.clone(),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(123u64, config_res.token_code_id);
    assert_eq!(msg.pair_configs, config_res.pair_configs);
    assert_eq!(Addr::unchecked(owner), config_res.owner);
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let pair_configs = vec![PairConfig {
        code_id: 123u64,
        pair_type: PairType::Xyk {},
        total_fee_bps: 3,
        maker_fee_bps: 166,
        is_disabled: false,
        is_generator_disabled: false,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id: 123u64,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Update config
    let env = mock_env();
    let info = mock_info(owner, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        token_code_id: Some(200u64),
        fee_address: Some(String::from("new_fee_addr")),
        generator_address: Some(String::from("new_generator_addr")),
        whitelist_code_id: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(200u64, config_res.token_code_id);
    assert_eq!(owner, config_res.owner);
    assert_eq!(
        String::from("new_fee_addr"),
        config_res.fee_address.unwrap()
    );
    assert_eq!(
        String::from("new_generator_addr"),
        config_res.generator_address.unwrap()
    );

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        token_code_id: None,
        fee_address: None,
        generator_address: None,
        whitelist_code_id: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});
}

#[test]
fn update_owner() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let msg = InstantiateMsg {
        pair_configs: vec![],
        token_code_id: 123u64,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
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
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
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
        from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(new_owner, config.owner);
}

#[test]
fn update_pair_config() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";
    let pair_configs = vec![PairConfig {
        code_id: 123u64,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: false,
        is_generator_disabled: false,
    }];

    let msg = InstantiateMsg {
        pair_configs: pair_configs.clone(),
        token_code_id: 123u64,
        fee_address: None,
        owner: owner.to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(pair_configs, config_res.pair_configs);

    // Update config
    let pair_config = PairConfig {
        code_id: 800,
        pair_type: PairType::Xyk {},
        total_fee_bps: 1,
        maker_fee_bps: 2,
        is_disabled: false,
        is_generator_disabled: false,
    };

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("wrong-addr0000", &[]);
    let msg = ExecuteMsg::UpdatePairConfig {
        config: pair_config.clone(),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Check validation of total and maker fee bps
    let env = mock_env();
    let info = mock_info(owner.clone(), &[]);
    let msg = ExecuteMsg::UpdatePairConfig {
        config: PairConfig {
            code_id: 123u64,
            pair_type: PairType::Xyk {},
            total_fee_bps: 3,
            maker_fee_bps: 10_001,
            is_disabled: false,
            is_generator_disabled: false,
        },
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PairConfigInvalidFeeBps {});

    let info = mock_info(owner.clone(), &[]);
    let msg = ExecuteMsg::UpdatePairConfig {
        config: pair_config.clone(),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(vec![pair_config.clone()], config_res.pair_configs);

    // Add second config
    let pair_config_custom = PairConfig {
        code_id: 100,
        pair_type: PairType::Custom("test".to_string()),
        total_fee_bps: 10,
        maker_fee_bps: 20,
        is_disabled: false,
        is_generator_disabled: false,
    };

    let info = mock_info(owner.clone(), &[]);
    let msg = ExecuteMsg::UpdatePairConfig {
        config: pair_config_custom.clone(),
    };

    execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_binary(&query_res).unwrap();
    assert_eq!(
        vec![pair_config_custom.clone(), pair_config.clone()],
        config_res.pair_configs
    );
}

#[test]
fn create_pair() {
    let mut deps = mock_dependencies(&[]);

    let pair_config = PairConfig {
        code_id: 321u64,
        pair_type: PairType::Xyk {},
        total_fee_bps: 100,
        maker_fee_bps: 10,
        is_disabled: false,
        is_generator_disabled: false,
    };

    let msg = InstantiateMsg {
        pair_configs: vec![pair_config.clone()],
        token_code_id: 123u64,
        fee_address: None,
        owner: "owner0000".to_string(),
        generator_address: Some(String::from("generator")),
        whitelist_code_id: 234u64,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
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

    // Check pair creation using a non-whitelisted pair ID
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::CreatePair {
            pair_type: PairType::Stable {},
            asset_infos: asset_infos.clone(),
            init_params: None,
        },
    )
    .unwrap_err();
    assert_eq!(res, ContractError::PairConfigNotFound {});

    let res = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::CreatePair {
            pair_type: PairType::Xyk {},
            asset_infos: asset_infos.clone(),
            init_params: None,
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
                    factory_addr: String::from(MOCK_CONTRACT_ADDR),
                    asset_infos: asset_infos.clone(),
                    token_code_id: msg.token_code_id,
                    init_params: None
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
        pair_configs: vec![PairConfig {
            code_id: 123u64,
            pair_type: PairType::Xyk {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id: 123u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.to_string(),
        whitelist_code_id: 234u64,
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
        pair_type: PairType::Xyk {},
        asset_infos: asset_infos.clone(),
        init_params: None,
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

    // Register an Astroport pair querier
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
        result: SubMsgResult::Ok(SubMsgResponse {
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

    // Check pair was registered
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
        pair_type: PairType::Xyk {},
        asset_infos: asset_infos_2.clone(),
        init_params: None,
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

    // Register astroport pair querier
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
        result: SubMsgResult::Ok(SubMsgResponse {
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
