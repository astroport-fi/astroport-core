#![cfg(not(tarpaulin_include))]

use cosmwasm_std::{Addr, Uint128};

use astroport::asset::{native_asset, AssetInfoExt};
use astroport::pair::{StablePoolParams, XYKPoolParams};
use astroport_liquidity_manager::error::ContractError;

use crate::helper::{f64_to_dec, Helper, PoolParams, TestCoin};

mod helper;

#[test]
fn test_xyk() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("TEST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Constant(XYKPoolParams {
            track_asset_balances: None,
        }),
    )
    .unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            Some(Uint128::MIN), // setting zero just to make initial provision via manager contract,
        )
        .unwrap();

    let user1 = Addr::unchecked("user1");
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&provide_assets, &user1);
    let sim_lp_amount = helper.simulate_provide(None, &provide_assets).unwrap();
    helper
        .provide_liquidity(&user1, &provide_assets, Some(sim_lp_amount))
        .unwrap();

    // Imagine user3 wants to inject a huge swap which imbalance pool right before victims' (user2) usual provide.
    let user3 = Addr::unchecked("user3");
    let swap_asset = helper.assets[&test_coins[1]].with_balance(10_000_000000_u128);
    helper.give_me_money(&[swap_asset.clone()], &user3);
    helper
        .swap(&user3, &swap_asset, Some(f64_to_dec(0.5)))
        .unwrap();

    let user2 = Addr::unchecked("user2");
    helper.give_me_money(&provide_assets, &user2);
    // User2 expects that he is making balanced provide (directly in pair contract). Allowing only 2% slippage.
    let err = helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            None,
            false,
            None,
        )
        .unwrap_err();

    // However, he is safe because of slippage assertion.
    assert_eq!(
        astroport_pair::error::ContractError::MaxSlippageAssertion {},
        err.downcast().unwrap()
    );

    // User intentionally allowing 50% slippage. But he is providing via liquidity manager contract.
    let slippage_tol = Some(f64_to_dec(0.5));
    let sim_lp_amount = helper
        .simulate_provide(slippage_tol, &provide_assets)
        .unwrap();
    helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            slippage_tol,
            Some(sim_lp_amount),
            false,
            None,
        )
        .unwrap();

    helper
        .withdraw_liquidity(&user2, helper.token_balance(&helper.lp_token, &user2), None)
        .unwrap();
    let asset1_bal = helper.coin_balance(&test_coins[0], &user2);
    let asset2_bal = helper.coin_balance(&test_coins[1], &user2);

    // After withdraw user2 should have nearly equal amount of assets as before he provided (minus rounding errors).
    assert_eq!(asset1_bal, 99_984289);
    assert_eq!(asset2_bal, 99_999998);

    // Lets check same scenario but without using liquidity manager contract.
    let user4 = Addr::unchecked("user4");
    helper.give_me_money(&provide_assets, &user4);
    helper
        .provide_liquidity_with_slip_tolerance(
            &user4,
            &provide_assets,
            slippage_tol,
            None,
            false,
            None,
        )
        .unwrap();

    helper
        .withdraw_liquidity(&user4, helper.token_balance(&helper.lp_token, &user4), None)
        .unwrap();
    let asset1_bal = helper.coin_balance(&test_coins[0], &user4);
    let asset2_bal = helper.coin_balance(&test_coins[1], &user4);

    // After withdraw user4 received much less asset2 because of unfair LP minting policy
    assert_eq!(asset1_bal, 82_687765);
    assert_eq!(asset2_bal, 99_999999);

    // User5 tries to fool liquidity manager out by providing assets with different order in message
    let user5 = Addr::unchecked("user5");
    let mut provide_assets_imbalanced = [
        helper.assets[&test_coins[0]].with_balance(10_000000u128),
        helper.assets[&test_coins[1]].with_balance(8_000000u128),
    ];
    helper.give_me_money(&provide_assets_imbalanced, &user5);

    let sim_lp_amount = helper
        .simulate_provide(slippage_tol, &provide_assets_imbalanced)
        .unwrap();

    // Changing order
    provide_assets_imbalanced.swap(0, 1);
    helper
        .provide_liquidity_with_slip_tolerance(
            &user5,
            &provide_assets_imbalanced,
            Some(f64_to_dec(0.5)),
            Some(sim_lp_amount),
            false,
            None,
        )
        .unwrap();

    helper
        .withdraw_liquidity(&user5, helper.token_balance(&helper.lp_token, &user5), None)
        .unwrap();
    let asset1_bal = helper.coin_balance(&test_coins[0], &user5);
    let asset2_bal = helper.coin_balance(&test_coins[1], &user5);

    // However this trick doesn't work with liquidity manager contract.
    // User5 received nearly equal amount of assets as provided minus fees charged due to imbalanced provide.
    assert_eq!(asset1_bal, 9_999753);
    assert_eq!(asset2_bal, 7_999998);
}

#[test]
fn test_stableswap_without_manager() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("UST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Stable(StablePoolParams {
            amp: 40,
            owner: None,
        }),
    )
    .unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            None,
        )
        .unwrap();

    let user1 = Addr::unchecked("user1");
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&provide_assets, &user1);
    let sim_lp_amount = helper.simulate_provide(None, &provide_assets).unwrap();
    helper
        .provide_liquidity(&user1, &provide_assets, Some(sim_lp_amount))
        .unwrap();

    // Malicious user3 becomes major LP holder.
    let user3 = Addr::unchecked("user3");
    let imbalanced_provide = [
        helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
    ];
    helper.give_me_money(&imbalanced_provide, &user3);
    helper
        .provide_liquidity(&user3, &imbalanced_provide, None)
        .unwrap();
    // User3 imbalances pool right before the victim's provide.
    let swap_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000_u128);
    helper.give_me_money(&[swap_asset.clone()], &user3);
    helper
        .swap(&user3, &swap_asset, Some(f64_to_dec(0.5)))
        .unwrap();

    let user2 = Addr::unchecked("user2");
    helper.give_me_money(&provide_assets, &user2);
    // User2 expects that he is making balanced provide (directly in pair contract). However, stableswap doesn't have slippage check.
    helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            None,
            false,
            None,
        )
        .unwrap();

    // user2 receives less LP tokens than expected
    let user2_lp_bal = helper.token_balance(&helper.lp_token, &user2);
    assert!(
        user2_lp_bal < sim_lp_amount.u128(),
        "user2 lp balance {user2_lp_bal} should be less than simulated: {sim_lp_amount}"
    );
}

#[test]
fn test_stableswap_with_manager() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("UST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Stable(StablePoolParams {
            amp: 40,
            owner: None,
        }),
    )
    .unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            None,
        )
        .unwrap();

    // Simulating LP tokens amount before provide
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    let sim_lp_amount = helper.simulate_provide(None, &provide_assets).unwrap();

    // Malicious user3 becomes major LP holder.
    let user3 = Addr::unchecked("user3");
    let malicious_provide = [
        helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
        helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
    ];
    helper.give_me_money(&malicious_provide, &user3);
    helper
        .provide_liquidity(&user3, &malicious_provide, None)
        .unwrap();
    // User3 imbalances pool right before the victim's provide.
    let swap_asset = helper.assets[&test_coins[0]].with_balance(10_000_000000_u128);
    helper.give_me_money(&[swap_asset.clone()], &user3);
    helper
        .swap(&user3, &swap_asset, Some(f64_to_dec(0.5)))
        .unwrap();

    let user2 = Addr::unchecked("user2");
    // User2 expects that he is making balanced provide and he uses liquidity manager contract.
    helper.give_me_money(&provide_assets, &user2);
    let err = helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            Some(sim_lp_amount),
            false,
            None,
        )
        .unwrap_err();

    assert_eq!(
        ContractError::ProvideSlippageViolation(199_998620u128.into(), 200_000000u128.into()),
        err.downcast().unwrap()
    );
}

#[test]
fn test_auto_stake_and_receiver() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("UST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Stable(StablePoolParams {
            amp: 40,
            owner: None,
        }),
    )
    .unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            None,
        )
        .unwrap();

    // Simulating LP tokens amount before provide
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    let sim_lp_amount = helper.simulate_provide(None, &provide_assets).unwrap();

    let user2 = Addr::unchecked("user2");
    helper.give_me_money(&provide_assets, &user2);
    // Providing with auto stake
    helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            Some(sim_lp_amount),
            true,
            None,
        )
        .unwrap();

    let lp_bal = helper.query_staked_lp(&user2).unwrap();
    assert_eq!(lp_bal, sim_lp_amount);

    let user3 = Addr::unchecked("user3");

    helper.give_me_money(&provide_assets, &user2);
    // Providing with auto stake and different receiver
    helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            Some(sim_lp_amount),
            true,
            Some(user3.to_string()),
        )
        .unwrap();

    let lp_bal = helper.query_staked_lp(&user3).unwrap();
    assert_eq!(lp_bal, sim_lp_amount);

    helper.give_me_money(&provide_assets, &user2);
    // Providing without auto stake but with different receiver
    helper
        .provide_liquidity_with_slip_tolerance(
            &user2,
            &provide_assets,
            Some(f64_to_dec(0.02)),
            Some(sim_lp_amount),
            false,
            Some(user3.to_string()),
        )
        .unwrap();

    let lp_bal = helper.token_balance(&helper.lp_token, &user3);
    assert_eq!(lp_bal, sim_lp_amount.u128());
}

#[test]
fn test_withdraw() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uluna"), TestCoin::cw20("TEST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Constant(XYKPoolParams {
            track_asset_balances: None,
        }),
    )
    .unwrap();

    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            None,
        )
        .unwrap();

    let owner_lp_bal = helper.token_balance(&helper.lp_token, &owner);
    let mut sim_withdraw = helper.simulate_withdraw(owner_lp_bal).unwrap();

    // user makes swap and imbalance pool after owner's withdraw simulation
    let user = Addr::unchecked("user");
    let swap_asset = helper.assets[&test_coins[0]].with_balance(1000_000000_u128);
    helper.give_me_money(&[swap_asset.clone()], &user);
    helper
        .swap(&user, &swap_asset, Some(f64_to_dec(0.5)))
        .unwrap();

    let err = helper
        .withdraw_liquidity(&owner, owner_lp_bal, Some(sim_withdraw.clone()))
        .unwrap_err();
    assert_eq!(
        ContractError::WithdrawSlippageViolation {
            asset_name: helper.assets[&test_coins[1]].to_string(),
            received: 99011_385149u128.into(),
            expected: 99999_999000u128.into(),
        },
        err.downcast().unwrap()
    );

    // Relaxing slippage tolerance
    sim_withdraw[1].amount = 99000_000000u128.into();
    helper
        .withdraw_liquidity(&owner, owner_lp_bal, Some(sim_withdraw))
        .unwrap();

    // Check withdraw with wrong number of assets fails
    let provide_assets = [
        helper.assets[&test_coins[0]].with_balance(100_000000u128),
        helper.assets[&test_coins[1]].with_balance(100_000000u128),
    ];
    helper.give_me_money(&provide_assets, &user);
    helper
        .provide_liquidity(&user, &provide_assets, None)
        .unwrap();

    let user_lp_bal = helper.token_balance(&helper.lp_token, &user);
    let mut sim_withdraw = helper.simulate_withdraw(user_lp_bal).unwrap();
    sim_withdraw.pop();
    let err = helper
        .withdraw_liquidity(&user, user_lp_bal, Some(sim_withdraw.clone()))
        .unwrap_err();
    assert_eq!(
        ContractError::WrongAssetLength {
            expected: 2,
            actual: 1,
        },
        err.downcast().unwrap()
    );

    // Adding random asset which doesn't exist in the pool
    sim_withdraw.push(native_asset("random".to_string(), 1000_000000u128.into()));
    let err = helper
        .withdraw_liquidity(&user, user_lp_bal, Some(sim_withdraw))
        .unwrap_err();
    assert_eq!(
        ContractError::AssetNotInPair("random".to_string()),
        err.downcast().unwrap()
    );
}

#[test]
fn test_onesided_provide_stable() {
    let owner = Addr::unchecked("owner");
    let test_coins = vec![TestCoin::native("uusd"), TestCoin::cw20("UST")];
    let mut helper = Helper::new(
        &owner,
        test_coins.clone(),
        PoolParams::Stable(StablePoolParams {
            amp: 40,
            owner: None,
        }),
    )
    .unwrap();

    // initial provide must be double-sided
    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(100_000_000000_u128),
            ],
            None,
        )
        .unwrap();

    // one-sided provide
    helper
        .provide_liquidity(
            &owner,
            &[
                helper.assets[&test_coins[0]].with_balance(100_000_000000_u128),
                helper.assets[&test_coins[1]].with_balance(0u8),
            ],
            None,
        )
        .unwrap();
}
