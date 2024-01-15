use cosmwasm_std::Addr;
use cw_multi_test::Executor;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, ExecuteMsg, QueryMsg, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport_pair_transmuter::error::ContractError;

use crate::helper::{Helper, TestCoin};

mod helper;

#[test]
fn test_instantiate() {
    let owner = Addr::unchecked("owner");

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("usdt"), TestCoin::cw20("USDC")],
    )
    .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Cw20TokenNotSupported {}
    );

    let err = Helper::new(&owner, vec![TestCoin::native("usdt")]).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidAssetLength {}
    );

    let err = Helper::new(
        &owner,
        vec![TestCoin::native("usdt"), TestCoin::native("usdt")],
    )
    .unwrap_err();
    assert_eq!(
        err.downcast::<astroport_factory::error::ContractError>()
            .unwrap(),
        astroport_factory::error::ContractError::DoublingAssets {}
    );

    Helper::new(
        &owner,
        vec![TestCoin::native("usdt"), TestCoin::native("usdc")],
    )
    .unwrap();
}

#[test]
fn test_provide_and_withdraw() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("usdt"), TestCoin::native("usdc")];

    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    let user = Addr::unchecked("user");
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
    ];

    helper.give_me_money(&provide_assets, &user);

    helper.provide_liquidity(&user, &provide_assets).unwrap();

    let lp_balance = helper.token_balance(&helper.lp_token, &user);
    assert_eq!(lp_balance, 2 * 100_000_000000u128);

    // withdraw half. balanced
    helper
        .withdraw_liquidity(&user, 100_000_000000u128, vec![])
        .unwrap();

    let lp_balance = helper.token_balance(&helper.lp_token, &user);
    assert_eq!(lp_balance, 100_000_000000u128);

    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(50_000_000000u128),
            helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
        ]
    );

    assert_eq!(
        helper.coin_balance(&test_coins[0], &user),
        50_000_000000u128
    );
    assert_eq!(
        helper.coin_balance(&test_coins[1], &user),
        50_000_000000u128
    );

    // withdraw imbalanced
    helper
        .withdraw_liquidity(
            &user,
            50_000_000000u128,
            vec![helper.assets[&test_coins[0]].with_balance(50_000_000000u128)],
        )
        .unwrap();

    assert_eq!(
        helper.coin_balance(&test_coins[0], &user),
        100_000_000000u128
    );

    // LP tokens left
    assert_eq!(
        helper.token_balance(&helper.lp_token, &user),
        50_000_000000u128
    );

    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(0u128),
            helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
        ]
    );

    // Try withdraw from empty pool
    let err = helper
        .withdraw_liquidity(
            &user,
            5_000_000000u128,
            vec![helper.assets[&test_coins[0]].with_balance(5_000_000000u128)],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InsufficientPoolBalance {
            asset: helper.assets[&test_coins[0]].to_string(),
            want: 5_000_000000u128.into(),
            available: 0u128.into(),
        }
    );

    // Try withdraw unknown token
    let err = helper
        .withdraw_liquidity(
            &user,
            5_000_000000u128,
            vec![Asset::native("unknown", 5_000_000000u128)],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidAsset("unknown".to_string())
    );

    // Try withdraw more than available
    let err = helper
        .withdraw_liquidity(
            &user,
            5_000_000000u128,
            vec![helper.assets[&test_coins[1]].with_balance(10_000_000000u128)],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InsufficientLpTokens {
            required: 10_000_000000u128.into(),
            available: 5_000_000000u128.into()
        }
    );

    // Supply more LP tokens than required
    helper
        .withdraw_liquidity(
            &user,
            10_000_000000u128,
            vec![helper.assets[&test_coins[1]].with_balance(5_000_000000u128)],
        )
        .unwrap();

    // 5k LP tokens returned to user balance
    assert_eq!(
        helper.token_balance(&helper.lp_token, &user),
        45_000_000000u128
    );

    // imbalanced provide
    let user = Addr::unchecked("user2");
    let provide_assets = [helper.assets[&test_coins[0]].with_balance(10_000_000000u128)];
    helper.give_me_money(&provide_assets, &user);

    helper.provide_liquidity(&user, &provide_assets).unwrap();

    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(10_000_000000u128),
            helper.assets[&test_coins[1]].with_balance(45_000_000000u128),
        ]
    );
}

#[test]
fn test_swap() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("usdt"), TestCoin::native("usdc")];

    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
            ],
        )
        .unwrap();

    let user = Addr::unchecked("user");
    let swap_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000u128);
    helper.give_me_money(&[swap_asset.clone()], &user);
    helper.swap(&user, &swap_asset, None, None).unwrap();

    assert_eq!(helper.coin_balance(&test_coins[0], &user), 0);
    assert_eq!(
        helper.coin_balance(&test_coins[1], &user),
        10_000_000000u128
    );
    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(110_000_000000u128),
            helper.assets[&test_coins[1]].with_balance(90_000_000000u128),
        ]
    );

    let user = Addr::unchecked("user2");
    let swap_asset = helper.assets[&test_coins[0]].with_balance(91_000_000000u128);
    helper.give_me_money(&[swap_asset.clone()], &user);
    let err = helper.swap(&user, &swap_asset, None, None).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InsufficientPoolBalance {
            asset: helper.assets[&test_coins[1]].to_string(),
            want: 91_000_000000u128.into(),
            available: 90_000_000000u128.into(),
        }
    );
}

#[test]
fn test_multipool_swap() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![
        TestCoin::native("usdc.axl"),
        TestCoin::native("usdc.eth"),
        TestCoin::native("usdc"),
    ];

    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
                helper.assets[&test_coins[2]].with_balance(100_000_000000u128),
            ],
        )
        .unwrap();

    let user = Addr::unchecked("user");
    let swap_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000u128);
    helper.give_me_money(&[swap_asset.clone()], &user);

    let err = helper.swap(&user, &swap_asset, None, None).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AskAssetMustBeSet {}
    );

    helper
        .swap(
            &user,
            &swap_asset,
            Some(helper.assets[&test_coins[2]].clone()),
            None,
        )
        .unwrap();

    assert_eq!(helper.coin_balance(&test_coins[0], &user), 0);
    assert_eq!(
        helper.coin_balance(&test_coins[2], &user),
        10_000_000000u128
    );
    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(110_000_000000u128),
            helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
            helper.assets[&test_coins[2]].with_balance(90_000_000000u128),
        ]
    );

    let user = Addr::unchecked("user2");
    let swap_asset = helper.assets[&test_coins[0]].with_balance(101_000_000000u128);
    helper.give_me_money(&[swap_asset.clone()], &user);
    let err = helper
        .swap(
            &user,
            &swap_asset,
            Some(helper.assets[&test_coins[1]].clone()),
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InsufficientPoolBalance {
            asset: helper.assets[&test_coins[1]].to_string(),
            want: 101_000_000000u128.into(),
            available: 100_000_000000u128.into(),
        }
    );

    // withdraw imbalanced
    helper
        .withdraw_liquidity(
            &owner,
            100_000_000000u128,
            vec![helper.assets[&test_coins[1]].with_balance(100_000_000000u128)],
        )
        .unwrap();

    let pool_info = helper.query_pool().unwrap();
    assert_eq!(
        pool_info.assets,
        vec![
            helper.assets[&test_coins[0]].with_balance(110_000_000000u128),
            helper.assets[&test_coins[1]].with_balance(0u128),
            helper.assets[&test_coins[2]].with_balance(90_000_000000u128),
        ]
    );
}

#[test]
fn test_provide_liquidity_without_funds() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("usdc"), TestCoin::native("usdc.axl")];

    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    let user1 = Addr::unchecked("user1");

    let assets = vec![
        helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
        helper.assets[&test_coins[1]].with_balance(50_000_000000u128),
    ];

    // provide some liquidity
    for _ in 0..3 {
        helper.give_me_money(&assets, &user1);
        helper.provide_liquidity(&user1, &assets).unwrap();
    }

    let msg = ExecuteMsg::ProvideLiquidity {
        assets: assets.clone().to_vec(),
        slippage_tolerance: None,
        auto_stake: None,
        receiver: None,
    };

    let err = helper
        .app
        .execute_contract(user1.clone(), helper.pair_addr.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Native token balance mismatch between the argument (100000000000usdc) and the transferred (0usdc)"
    );

    // Test unsupported msg as well
    let msg = ExecuteMsg::DropOwnershipProposal {};

    let err = helper
        .app
        .execute_contract(user1.clone(), helper.pair_addr.clone(), &msg, &[])
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NotSupported {}
    )
}

#[test]
fn test_queries() {
    let owner = Addr::unchecked("owner");

    let test_coins = vec![TestCoin::native("usdt"), TestCoin::native("usdc")];

    let mut helper = Helper::new(&owner, test_coins.clone()).unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000u128),
            ],
        )
        .unwrap();

    let pool_info = helper.query_config().unwrap();
    assert_eq!(
        pool_info,
        ConfigResponse {
            block_time_last: 0,
            params: None,
            owner: owner.clone(),
            factory_addr: helper.factory.clone(),
        }
    );

    let share = helper.query_share(2_000000u128).unwrap();
    assert_eq!(
        share,
        [
            helper.assets[&test_coins[0]].with_balance(1_000000u128),
            helper.assets[&test_coins[1]].with_balance(1_000000u128),
        ]
    );

    let sim_res = helper
        .simulate_swap(
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
            None,
        )
        .unwrap();
    assert_eq!(
        sim_res,
        SimulationResponse {
            return_amount: 1_000000u128.into(),
            spread_amount: Default::default(),
            commission_amount: Default::default(),
        }
    );

    // Erroneous queries
    let err = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
            Some(AssetInfo::native("test")),
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: The asset test does not belong to the pair"
    );

    let err = helper
        .simulate_reverse_swap(&AssetInfo::native("test").with_balance(1u128), None)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: The asset test does not belong to the pair"
    );

    let err = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[0]].with_balance(110_000_000000u128),
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Insufficient pool usdt balance. Want: 110000000000, available: 100000000000"
    );

    let sim_res = helper
        .simulate_reverse_swap(
            &helper.assets[&test_coins[0]].with_balance(1_000000u128),
            None,
        )
        .unwrap();
    assert_eq!(
        sim_res,
        ReverseSimulationResponse {
            offer_amount: 1_000000u128.into(),
            spread_amount: Default::default(),
            commission_amount: Default::default(),
        }
    );

    // Unsupported query
    let err = helper
        .app
        .wrap()
        .query_wasm_smart::<CumulativePricesResponse>(
            helper.pair_addr.clone(),
            &QueryMsg::CumulativePrices {},
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Endpoint is not supported"
    );
}
