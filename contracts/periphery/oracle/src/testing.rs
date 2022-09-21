use crate::contract::{execute, instantiate};
use crate::mock_querier::mock_dependencies;
use astroport::asset::{Asset, AssetInfo};
use astroport::oracle::{ExecuteMsg, InstantiateMsg};
use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{Addr, Decimal256, Uint128, Uint256};
use std::ops::Mul;

#[test]
fn decimal_overflow() {
    let price_cumulative_current = Uint128::from(100u128);
    let price_cumulative_last = Uint128::from(192738282u128);
    let time_elapsed: u64 = 86400;
    let amount = Uint128::from(1000u128);
    let price_average = Decimal256::from_ratio(
        Uint256::from(price_cumulative_current.wrapping_sub(price_cumulative_last)),
        time_elapsed,
    );

    println!("{}", price_average.to_string());

    let res: Uint128 = price_average.mul(Uint256::from(amount)).try_into().unwrap();
    println!("{}", res);
}

#[test]
fn oracle_overflow() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let mut env = mock_env();
    let factory = Addr::unchecked("factory");
    let astro_token_contract = Addr::unchecked("astro-token");
    let usdc_token_contract = Addr::unchecked("usdc-token");

    let astro_asset_info = AssetInfo::Token {
        contract_addr: astro_token_contract.clone(),
    };
    let usdc_asset_info = AssetInfo::Token {
        contract_addr: usdc_token_contract.clone(),
    };
    let astro_asset = Asset {
        info: astro_asset_info.clone(),
        amount: Uint128::zero(),
    };
    let usdc_asset = Asset {
        info: usdc_asset_info.clone(),
        amount: Uint128::zero(),
    };

    let asset = [astro_asset.clone(), usdc_asset.clone()];

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        asset_infos: [astro_asset_info, usdc_asset_info],
    };

    // Set cumulative price to 192738282u128
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset.clone(),
        Uint128::from(192738282u128),
        Uint128::from(192738282u128),
        Uint128::from(192738282u128),
    );
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
    // Set cumulative price to 100 (overflow)
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset,
        Uint128::from(100u128),
        Uint128::from(100u128),
        Uint128::from(100u128),
    );
    env.block.time = env.block.time.plus_seconds(86400);
    execute(deps.as_mut(), env, info, ExecuteMsg::Update {}).unwrap();
}
