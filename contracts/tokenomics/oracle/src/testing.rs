use crate::contract::instantiate;
use crate::mock_querier::{mock_dependencies, AstroMockQuerier};
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::CONFIG;
use astroport::asset::AssetInfo;
use cosmwasm_std::testing::{mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, Addr, BlockInfo, ContractInfo, Decimal, Env, OwnedDeps, Timestamp,
};

struct MockEnvParams {
    pub block_time: Timestamp,
    pub block_height: u64,
}

impl Default for MockEnvParams {
    fn default() -> Self {
        MockEnvParams {
            block_time: Timestamp::from_nanos(1_571_797_419_879_305_533),
            block_height: 1,
        }
    }
}

fn mock_env(mock_env_params: MockEnvParams) -> Env {
    Env {
        block: BlockInfo {
            height: mock_env_params.block_height,
            time: mock_env_params.block_time,
            chain_id: "cosmos-testnet-14002".to_string(),
        },
        contract: ContractInfo {
            address: Addr::unchecked(MOCK_CONTRACT_ADDR),
        },
    }
}

// fn th_setup() -> OwnedDeps<MockStorage, MockApi, AstroMockQuerier> {
//     let mut deps = mock_dependencies(&[]);
//
//     let msg = InstantiateMsg {
//         owner: String::from("owner"),
//     };
//     let info = mock_info("owner", &[]);
//     instantiate(deps.as_mut(), mock_env(MockEnvParams::default()), info, msg).unwrap();
//
//     deps
// }

#[test]
fn test_proper_init() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        factory: "factory".to_string(),
        asset_infos: [
            AssetInfo::Token {
                contract_addr: Addr::unchecked("astro"),
            },
            AssetInfo::Token {
                contract_addr: Addr::unchecked("luna"),
            },
        ],
    };
    let info = mock_info("owner", &[]);

    let res = instantiate(deps.as_mut(), mock_env(MockEnvParams::default()), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(Addr::unchecked("owner"), config.owner);
}

// #[test]
// fn test_update_config() {
//     let mut deps = th_setup();
//     let env = mock_env(MockEnvParams::default());
//
//     // only owner can update
//     {
//         let msg = ExecuteMsg::UpdateConfig {
//             owner: Some(String::from("new_owner")),
//         };
//         let info = mock_info("another_one", &[]);
//         let err = execute(deps.as_mut(), env.clone(), info, msg);
//         match err {
//             Ok(_) => panic!("Must return error"),
//             Err(ContractError::Unauthorized { .. }) => {}
//             Err(e) => panic!("Unexpected error: {:?}", e),
//         }
//     }
//
//     let info = mock_info("owner", &[]);
//     // no change
//     {
//         let msg = ExecuteMsg::UpdateConfig { owner: None };
//         execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
//
//         let config = CONFIG.load(&deps.storage).unwrap();
//         assert_eq!(config.owner, Addr::unchecked("owner"));
//     }
//
//     // new owner
//     {
//         let msg = ExecuteMsg::UpdateConfig {
//             owner: Some(String::from("new_owner")),
//         };
//         execute(deps.as_mut(), env, info, msg).unwrap();
//
//         let config = CONFIG.load(&deps.storage).unwrap();
//         assert_eq!(config.owner, Addr::unchecked("new_owner"));
//     }
// }
//
// #[test]
// fn test_set_asset() {
//     let mut deps = th_setup();
//     let env = mock_env(MockEnvParams::default());
//
//     // only owner can set asset
//     {
//         let msg = ExecuteMsg::SetAssetInfo {
//             asset_info: AssetInfo::NativeToken {
//                 denom: "luna".to_string(),
//             },
//             price_source: PriceSourceUnchecked::Native {
//                 denom: "luna".to_string(),
//             },
//         };
//         let info = mock_info("another_one", &[]);
//         let err = execute(deps.as_mut(), env.clone(), info, msg);
//         match err {
//             Ok(_) => panic!("Must return error"),
//             Err(ContractError::Unauthorized { .. }) => {}
//             Err(e) => panic!("Unexpected error: {:?}", e),
//         }
//     }
//
//     let info = mock_info("owner", &[]);
//     // native
//     {
//         let asset = AssetInfo::NativeToken {
//             denom: String::from("luna"),
//         };
//         let reference = get_reference(&asset);
//         let msg = ExecuteMsg::SetAssetInfo {
//             asset_info: asset,
//             price_source: PriceSourceUnchecked::Native {
//                 denom: "luna".to_string(),
//             },
//         };
//         execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
//         let price_config = PRICE_CONFIGS
//             .load(&deps.storage, reference.as_slice())
//             .unwrap();
//         assert_eq!(
//             price_config.price_source,
//             PriceSourceChecked::Native {
//                 denom: "luna".to_string()
//             }
//         );
//     }
//
//     // cw20 terraswap
//     {
//         let asset = AssetInfo::Token {
//             contract_addr: Addr::unchecked("token"),
//         };
//         let reference = get_reference(&asset);
//         let msg = ExecuteMsg::SetAssetInfo {
//             asset_info: asset,
//             price_source: PriceSourceUnchecked::TerraswapUusdPair {
//                 pair_address: "token".to_string(),
//             },
//         };
//         execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
//         let price_config = PRICE_CONFIGS
//             .load(&deps.storage, reference.as_slice())
//             .unwrap();
//         assert_eq!(
//             price_config.price_source,
//             PriceSourceChecked::TerraswapUusdPair {
//                 pair_address: Addr::unchecked("token")
//             }
//         );
//     }
//
//     // cw20 fixed
//     {
//         let asset = AssetInfo::Token {
//             contract_addr: Addr::unchecked("token"),
//         };
//         let reference = get_reference(&asset);
//         let msg = ExecuteMsg::SetAssetInfo {
//             asset_info: asset,
//             price_source: PriceSourceUnchecked::Fixed {
//                 price: Decimal::from_ratio(1_u128, 2_u128),
//             },
//         };
//         execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
//         let price_config = PRICE_CONFIGS
//             .load(&deps.storage, reference.as_slice())
//             .unwrap();
//         assert_eq!(
//             price_config.price_source,
//             PriceSourceChecked::Fixed {
//                 price: Decimal::from_ratio(1_u128, 2_u128)
//             }
//         );
//     }
// }
//
// #[test]
// fn test_query_price_native() {
//     let mut deps = th_setup();
//     let asset = AssetInfo::NativeToken {
//         denom: String::from("nativecoin"),
//     };
//     let reference = get_reference(&asset);
//
//     deps.querier.set_native_exchange_rates(
//         "nativecoin".to_string(),
//         &[("uusd".to_string(), Decimal::from_ratio(4_u128, 1_u128))],
//     );
//
//     PRICE_CONFIGS
//         .save(
//             &mut deps.storage,
//             reference.as_slice(),
//             &PriceConfig {
//                 price_source: PriceSourceChecked::Native {
//                     denom: "nativecoin".to_string(),
//                 },
//             },
//         )
//         .unwrap();
//
//     let env = mock_env(MockEnvParams::default());
//     let query: Decimal = from_binary(
//         &query(
//             deps.as_ref(),
//             env,
//             QueryMsg::AssetPriceByReference {
//                 asset_reference: b"nativecoin".to_vec(),
//             },
//         )
//         .unwrap(),
//     )
//     .unwrap();
//
//     assert_eq!(query, Decimal::from_ratio(4_u128, 1_u128));
// }
//
// #[test]
// fn test_query_price_fixed() {
//     let mut deps = th_setup();
//     let asset = AssetInfo::Token {
//         contract_addr: Addr::unchecked("cw20token"),
//     };
//     let reference = get_reference(&asset);
//
//     PRICE_CONFIGS
//         .save(
//             &mut deps.storage,
//             reference.as_slice(),
//             &PriceConfig {
//                 price_source: PriceSourceChecked::Fixed {
//                     price: Decimal::from_ratio(3_u128, 2_u128),
//                 },
//             },
//         )
//         .unwrap();
//
//     let env = mock_env(MockEnvParams::default());
//     let query: Decimal = from_binary(
//         &query(
//             deps.as_ref(),
//             env,
//             QueryMsg::AssetPriceByReference {
//                 asset_reference: Addr::unchecked("cw20token").as_bytes().to_vec(),
//             },
//         )
//         .unwrap(),
//     )
//     .unwrap();
//
//     assert_eq!(query, Decimal::from_ratio(3_u128, 2_u128));
// }
