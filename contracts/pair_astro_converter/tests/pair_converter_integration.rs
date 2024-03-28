use cosmwasm_std::{Addr, StdError};
use cw_multi_test::{App, Executor};

use astroport::asset::{AssetInfo, AssetInfoExt, PairInfo};
use astroport::factory::PairType;
use astroport::pair;
use astroport::pair::ConfigResponse;
use astroport_pair_converter::error::ContractError;

use crate::helper::{token_contract, Helper, TestCoin};

mod helper;

#[test]
fn test_migrate_from_xyk() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::cw20("ASTRO"), TestCoin::native("ibc/tf_astro")];
    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    let (converter_addr, converter_pair_code_id) = helper
        .setup_converter(helper.assets[&test_coins[0]].clone(), "ibc/true_tf_astro")
        .unwrap();
    let migrate_msg = astroport_pair_converter::migration::MigrateMsg {
        converter_contract: converter_addr.to_string(),
    };
    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            converter_pair_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Pair doesn't have new ASTRO denom specified in the converter contract"
    );

    let (converter_addr, converter_pair_code_id) = helper
        .setup_converter(AssetInfo::cw20_unchecked("another_cw20"), "ibc/tf_astro")
        .unwrap();
    let migrate_msg = astroport_pair_converter::migration::MigrateMsg {
        converter_contract: converter_addr.to_string(),
    };
    let err = helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            converter_pair_code_id,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Pair doesn't have old ASTRO specified in the converter contract"
    );

    let (converter_addr, converter_pair_code_id) = helper
        .setup_converter(helper.assets[&test_coins[0]].clone(), "ibc/tf_astro")
        .unwrap();
    let migrate_msg = astroport_pair_converter::migration::MigrateMsg {
        converter_contract: converter_addr.to_string(),
    };
    helper
        .app
        .migrate_contract(
            owner.clone(),
            helper.pair_addr.clone(),
            &migrate_msg,
            converter_pair_code_id,
        )
        .unwrap();
}

#[test]
fn test_old_hub() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::cw20("ASTRO"), TestCoin::native("ibc/tf_astro")];
    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();
    helper.setup_converter_and_migrate(&test_coins[0], &test_coins[1]);

    // old -> new
    helper
        .swap(
            &owner,
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
        )
        .unwrap();

    // Try new -> old
    let err = helper
        .swap(
            &owner,
            &helper.assets[&test_coins[1]].with_balance(1_000000u128),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AssetMismatch {
            old: helper.assets[&test_coins[0]].to_string(),
            new: helper.assets[&test_coins[1]].to_string(),
        }
    );

    // Try to provide liquidity
    let err = helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(1_000000u128),
                helper.assets[&test_coins[1]].with_balance(1_000000u128),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NotSupported {}
    );
}

#[test]
fn test_outpost() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native("ibc/old_astro"),
        TestCoin::native("tf_astro"),
    ];
    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();
    helper.setup_converter_and_migrate(&test_coins[0], &test_coins[1]);

    // old -> new
    helper
        .swap(
            &owner,
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
        )
        .unwrap();

    // Try new -> old
    let err = helper
        .swap(
            &owner,
            &helper.assets[&test_coins[1]].with_balance(1_000000u128),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AssetMismatch {
            old: helper.assets[&test_coins[0]].to_string(),
            new: helper.assets[&test_coins[1]].to_string(),
        }
    );

    // Try to provide liquidity
    let err = helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(1_000000u128),
                helper.assets[&test_coins[1]].with_balance(1_000000u128),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NotSupported {}
    );
}

#[test]
fn test_queries() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::cw20("ASTRO"), TestCoin::native("ibc/tf_astro")];
    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();
    helper.setup_converter_and_migrate(&test_coins[0], &test_coins[1]);

    let share = helper.query_share(100000000u128).unwrap();
    assert_eq!(
        share,
        vec![
            helper.assets[&test_coins[0]].with_balance(0u8),
            helper.assets[&test_coins[1]].with_balance(0u8),
        ]
    );

    let pool_resp = helper.query_pool().unwrap();
    assert_eq!(
        pool_resp,
        pair::PoolResponse {
            assets: vec![
                helper.assets[&test_coins[0]].with_balance(0u8),
                helper.assets[&test_coins[1]].with_balance(0u8),
            ],
            total_share: 0u8.into(),
        }
    );

    let err = helper.query_prices().unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: Operation is not supported")
    );

    let pool_info = helper
        .app
        .wrap()
        .query_wasm_smart::<PairInfo>(&helper.pair_addr, &pair::QueryMsg::Pair {})
        .unwrap();
    assert_eq!(pool_info.pair_type, PairType::Xyk {});

    let config = helper
        .app
        .wrap()
        .query_wasm_smart::<ConfigResponse>(&helper.pair_addr, &pair::QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            block_time_last: 0,
            params: None,
            owner: owner.clone(),
            factory_addr: helper.factory.clone(),
        }
    );

    let resp = helper
        .simulate_swap(
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
            None,
        )
        .unwrap();
    assert_eq!(
        resp,
        pair::SimulationResponse {
            return_amount: 1_000000u128.into(),
            spread_amount: 0u128.into(),
            commission_amount: 0u128.into(),
        }
    );
    let err = helper
        .simulate_swap(
            &helper.assets[&test_coins[1]].with_balance(1_000000u128),
            None,
        )
        .unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: This pair swaps from old ASTRO (contract0) to new ASTRO only (ibc/tf_astro)")
    );

    let resp = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[1]].with_balance(1_000000u128),
            None,
        )
        .unwrap();
    assert_eq!(
        resp,
        pair::ReverseSimulationResponse {
            offer_amount: 1_000000u128.into(),
            spread_amount: 0u128.into(),
            commission_amount: 0u128.into(),
        }
    );
    let err = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
            None,
        )
        .unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: This pair swaps from old ASTRO (contract0) to new ASTRO only (ibc/tf_astro)")
    );
}

#[test]
#[should_panic(expected = "not implemented: astroport-pair-converter cannot be instantiated")]
fn test_cant_instantiate() {
    let mut app = App::default();

    let token_code_id = app.store_code(token_contract());
    let converter_pair_code_id = app.store_code(helper::converter_pair_contract());
    app.instantiate_contract(
        converter_pair_code_id,
        Addr::unchecked("owner"),
        &astroport::pair::InstantiateMsg {
            asset_infos: vec![
                AssetInfo::cw20_unchecked("astro_addr"),
                AssetInfo::native("ibc/tf_astro"),
            ],
            token_code_id,
            factory_addr: "factory".to_string(),
            init_params: None,
        },
        &[],
        "label",
        None,
    )
    .unwrap();
}
