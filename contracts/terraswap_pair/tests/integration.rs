//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
//! You can easily convert unit tests to integration tests as follows:
//! 1. Copy them over verbatim
//! 2. Then change
//!      let mut deps = mock_dependencies(20, &[]);
//!    to
//!      let mut deps = mock_instance(WASM, &[]);
//! 3. If you access raw storage, where ever you see something like:
//!      deps.storage.get(CONFIG_KEY).expect("no data stored");
//!    replace it with:
//!      deps.with_storage(|store| {
//!          let data = store.get(CONFIG_KEY).expect("no data stored");
//!          //...
//!      });
//! 4. Anywhere you see query(&deps, ...) you must replace it with query(&mut deps, ...)

use cosmwasm_std::{
    from_binary, Coin, Decimal, HandleResponse, HandleResult, HumanAddr, InitResponse, StdError,
};
use cosmwasm_vm::testing::{
    handle, init, mock_dependencies, mock_env, query, MockApi, MockQuerier, MockStorage,
};
use cosmwasm_vm::Instance;
use terraswap::{AssetInfo, PairInitMsg};
use terraswap_pair::msg::{
    ConfigAssetResponse, ConfigGeneralResponse, ConfigSwapResponse, HandleMsg, QueryMsg,
};

// This line will test the output of cargo wasm
static WASM: &[u8] = include_bytes!("../../../target/wasm32-unknown-unknown/release/terraswap_pair.wasm");
// You can uncomment this line instead to test productionified build from rust-optimizer
// static WASM: &[u8] = include_bytes!("../contract.wasm");

const DEFAULT_GAS_LIMIT: u64 = 500_000;

pub fn mock_instance(
    wasm: &[u8],
    contract_balance: &[Coin],
) -> Instance<MockStorage, MockApi, MockQuerier> {
    // TODO: check_wasm is not exported from cosmwasm_vm
    // let terra_features = features_from_csv("staking,terra");
    // check_wasm(wasm, &terra_features).unwrap();
    let deps = mock_dependencies(20, contract_balance);
    Instance::from_code(wasm, deps, DEFAULT_GAS_LIMIT).unwrap()
}

#[test]
fn proper_initialization() {
    let mut deps = mock_instance(WASM, &[]);

    let msg = PairInitMsg {
        owner: HumanAddr("addr0000".to_string()),
        commission_collector: HumanAddr("collector0000".to_string()),
        lp_commission: Decimal::permille(3),
        owner_commission: Decimal::permille(1),
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("asset0000"),
            },
        ],
        token_code_id: 10u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res: InitResponse = init(&mut deps, env, msg).unwrap();

    // cannot change it after post intialization
    let msg = HandleMsg::PostInitialize {};
    let env = mock_env("liquidity0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();

    // it worked, let's query the state
    let res = query(&mut deps, QueryMsg::ConfigGeneral {}).unwrap();
    let config_general: ConfigGeneralResponse = from_binary(&res).unwrap();
    assert_eq!("addr0000", config_general.owner.as_str());
    assert_eq!(
        "collector0000",
        config_general.commission_collector.as_str()
    );
    assert_eq!("liquidity0000", config_general.liquidity_token.as_str());

    let res = query(&mut deps, QueryMsg::ConfigAsset {}).unwrap();
    let config_asset: ConfigAssetResponse = from_binary(&res).unwrap();
    assert_eq!(
        config_asset.infos,
        [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("asset0000")
            }
        ]
    );

    let res = query(&mut deps, QueryMsg::ConfigSwap {}).unwrap();
    let config_swap: ConfigSwapResponse = from_binary(&res).unwrap();
    assert_eq!(Decimal::permille(3), config_swap.lp_commission);
    assert_eq!(Decimal::permille(1), config_swap.owner_commission);
}

#[test]
fn update_config() {
    let mut deps = mock_instance(WASM, &[]);
    let msg = PairInitMsg {
        owner: HumanAddr("addr0000".to_string()),
        commission_collector: HumanAddr("collector0000".to_string()),
        lp_commission: Decimal::permille(3),
        owner_commission: Decimal::permille(1),
        asset_infos: [
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("asset0000"),
            },
        ],
        token_code_id: 10u64,
        init_hook: None,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res: InitResponse = init(&mut deps, env, msg).unwrap();

    // cannot change it after post intialization
    let msg = HandleMsg::PostInitialize {};
    let env = mock_env("liquidity0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();

    // update owner
    let env = mock_env("addr0000", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr("addr0001".to_string())),
        lp_commission: None,
        owner_commission: None,
    };

    let res: HandleResponse = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let query_result = query(&mut deps, QueryMsg::ConfigGeneral {}).unwrap();
    let config_general: ConfigGeneralResponse = from_binary(&query_result).unwrap();
    assert_eq!("addr0001", config_general.owner.as_str());

    // Unauthorzied err
    let env = mock_env("addr0000", &[]);
    let msg = HandleMsg::UpdateConfig {
        owner: None,
        lp_commission: Some(Decimal::percent(1)),
        owner_commission: Some(Decimal::percent(2)),
    };

    let res: HandleResult = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Unauthorized { .. } => {}
        _ => panic!("Must return unauthorized error"),
    }
}
