use cosmwasm_std::{coin, Addr, StdError};

use astroport::asset::{Asset, PairInfo};
use astroport::factory::PairType;
use astroport::pair;
use astroport::pair::{ConfigResponse, CumulativePricesResponse};
use astroport_pair_xastro::error::ContractError;

use crate::helper::{Helper, ASTRO_DENOM};

mod helper;

#[test]
fn test_basic() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    // Check ASTRO -> xASTRO swap
    let offer_asset = Asset::native(ASTRO_DENOM, 100000u128);

    let sim_response = helper.simulate_swap(&offer_asset, None).unwrap();

    helper.swap(&owner, &offer_asset, None, None).unwrap();

    let xastro_bal = helper.native_balance(&helper.xastro_denom, &owner);

    assert_eq!(xastro_bal, sim_response.return_amount.u128());
    assert_eq!(xastro_bal, 100000u128 - 1000);

    // Check xASTRO -> ASTRO swap
    let offer_asset = Asset::native(&helper.xastro_denom, xastro_bal);
    let sim_response = helper.simulate_swap(&offer_asset, None).unwrap();

    let astro_bal_before = helper.native_balance(ASTRO_DENOM, &owner);
    helper.swap(&owner, &offer_asset, None, None).unwrap();
    let astro_bal_after = helper.native_balance(ASTRO_DENOM, &owner);

    assert_eq!(
        astro_bal_after - astro_bal_before,
        sim_response.return_amount.u128()
    );
    assert_eq!(astro_bal_after - astro_bal_before, 100000u128 - 1000);

    let user = Addr::unchecked("user");
    helper
        .mint_tokens(&user, &[coin(200000, ASTRO_DENOM)])
        .unwrap();
    helper
        .swap(&owner, &Asset::native(ASTRO_DENOM, 100000u128), None, None)
        .unwrap();

    let err = helper
        .provide_liquidity(
            &owner,
            &[
                Asset::native(ASTRO_DENOM, 100000u128),
                Asset::native(&helper.xastro_denom, 100000u128),
            ],
        )
        .unwrap_err();
    assert_eq!(ContractError::NotSupported {}, err.downcast().unwrap());

    helper.mint_tokens(&user, &[coin(100000, "rand")]).unwrap();
    let err = helper
        .swap(&user, &Asset::native("rand", 100000u128), None, None)
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidAsset("rand".to_string()),
        err.downcast().unwrap()
    );
}

#[test]
fn test_queries() {
    let owner = Addr::unchecked("owner");

    let mut helper = Helper::new(&owner).unwrap();

    let share = helper.query_share(100000000u128).unwrap();
    assert_eq!(
        share,
        vec![
            Asset::native(ASTRO_DENOM, 0u8),
            Asset::native(&helper.xastro_denom, 0u8),
        ]
    );

    let pool_resp = helper.query_pool().unwrap();
    assert_eq!(
        pool_resp,
        pair::PoolResponse {
            assets: vec![
                Asset::native(ASTRO_DENOM, 0u8),
                Asset::native(&helper.xastro_denom, 0u8)
            ],
            total_share: 0u8.into(),
        }
    );

    let err = helper
        .app
        .wrap()
        .query_wasm_smart::<CumulativePricesResponse>(
            &helper.pair_addr,
            &pair::QueryMsg::CumulativePrices {},
        )
        .unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err("Querier contract error: Operation is not supported")
    );

    let pool_info = helper
        .app
        .wrap()
        .query_wasm_smart::<PairInfo>(&helper.pair_addr, &pair::QueryMsg::Pair {})
        .unwrap();
    assert_eq!(
        pool_info.pair_type,
        PairType::Custom("pair_xastro".to_string())
    );

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
            tracker_addr: None,
        }
    );

    let err = helper
        .simulate_swap(&Asset::native(ASTRO_DENOM, 1u128), None)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Initial stake amount must be more than 1000"
    );

    let offer_asset = Asset::native(ASTRO_DENOM, 100000u128);
    helper.swap(&owner, &offer_asset, None, None).unwrap();

    let resp = helper
        .simulate_swap(&Asset::native(ASTRO_DENOM, 1_000000u128), None)
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
        .simulate_swap(&Asset::native(&helper.xastro_denom, 100_000000u128), None)
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Querier contract error: Invalid unstake amount. Want 100000000 but staking contract has only 100000");

    let resp = helper
        .simulate_reverse_swap(&Asset::native(ASTRO_DENOM, 1_000000u128), None)
        .unwrap();
    assert_eq!(
        resp,
        pair::ReverseSimulationResponse {
            offer_amount: 1_000000u128.into(),
            spread_amount: 0u128.into(),
            commission_amount: 0u128.into(),
        }
    );

    let resp = helper
        .simulate_reverse_swap(&Asset::native(&helper.xastro_denom, 10000u128), None)
        .unwrap();
    assert_eq!(
        resp,
        pair::ReverseSimulationResponse {
            offer_amount: 10000u128.into(),
            spread_amount: 0u128.into(),
            commission_amount: 0u128.into(),
        }
    );

    let err = helper
        .simulate_swap(&Asset::native("rand", 100_000000u128), None)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Invalid asset rand"
    );
    let err = helper
        .simulate_reverse_swap(&Asset::native("rand", 100_000000u128), None)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Invalid asset rand"
    );
}
