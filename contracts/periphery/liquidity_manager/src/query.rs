use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdError, StdResult, Uint128};

use astroport::asset::{Asset, PairInfo};
use astroport::factory::PairType;
use astroport::liquidity_manager::QueryMsg;
use astroport::pair::{ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg};
use astroport::querier::query_supply;
use astroport_pair::contract::get_share_in_assets;
use astroport_pcl_common::state::Precisions;

use crate::error::ContractError;
use crate::utils::{
    convert_pcl_config, convert_stable_config, pcl_provide_simulation,
    stableswap_provide_simulation, xyk_provide_simulation,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::SimulateProvide {
            pair_addr,
            pair_msg,
        } => simulate_provide(deps, env, pair_addr, pair_msg),
        QueryMsg::SimulateWithdraw {
            pair_addr,
            lp_tokens,
        } => simulate_withdraw(deps, pair_addr, lp_tokens),
    }
}

fn simulate_provide(
    deps: Deps,
    env: Env,
    pair_addr: String,
    msg: PairExecuteMsg,
) -> StdResult<Binary> {
    match msg {
        PairExecuteMsg::ProvideLiquidity {
            mut assets,
            slippage_tolerance,
            ..
        } => {
            if assets.len() != 2 {
                return Err(StdError::generic_err(format!(
                    "{}",
                    ContractError::WrongPoolLength {}
                )));
            }
            let pair_addr = deps.api.addr_validate(&pair_addr)?;
            let pair_info: PairInfo = deps
                .querier
                .query_wasm_smart(&pair_addr, &PairQueryMsg::Pair {})?;
            match &pair_info.pair_type {
                PairType::Xyk {} => {
                    let pools = pair_info.query_pools(&deps.querier, &pair_addr)?;

                    let mut predicted_lp_amount = xyk_provide_simulation(
                        deps.querier,
                        &pools,
                        &pair_info,
                        slippage_tolerance,
                        assets.clone(),
                    )
                    .map_err(|err| StdError::generic_err(format!("{err}")))?;

                    // Initial provide is always fair because initial LP dictates the price
                    if !pools[0].amount.is_zero() && !pools[1].amount.is_zero() {
                        if pools[0].info.ne(&assets[0].info) {
                            assets.swap(0, 1);
                        }

                        // Add user's deposits
                        let balances_with_deposit = pools
                            .clone()
                            .into_iter()
                            .zip(assets.iter())
                            .map(|(mut pool, asset)| {
                                pool.amount += asset.amount;
                                pool
                            })
                            .collect::<Vec<_>>();
                        let total_share = query_supply(&deps.querier, &pair_info.liquidity_token)?;
                        let accrued_share = get_share_in_assets(
                            &balances_with_deposit,
                            predicted_lp_amount,
                            total_share + predicted_lp_amount,
                        );

                        // Simulate provide again without excess tokens
                        predicted_lp_amount = xyk_provide_simulation(
                            deps.querier,
                            &pools,
                            &pair_info,
                            slippage_tolerance,
                            accrued_share,
                        )
                        .map_err(|err| StdError::generic_err(format!("{err}")))?;
                    }

                    to_json_binary(&predicted_lp_amount)
                }
                PairType::Stable {} => {
                    let pair_config_data = deps
                        .querier
                        .query_wasm_raw(pair_addr, b"config")?
                        .ok_or_else(|| StdError::generic_err("pair stable config not found"))?;
                    let pair_config = convert_stable_config(deps.querier, pair_config_data)?;
                    to_json_binary(
                        &stableswap_provide_simulation(
                            deps.querier,
                            env,
                            pair_config,
                            slippage_tolerance,
                            assets,
                        )
                        .map_err(|err| StdError::generic_err(format!("{err}")))?,
                    )
                }
                PairType::Custom(typ) if typ == "concentrated" => {
                    let balances = pair_info.query_pools(&deps.querier, &pair_addr)?;
                    let pcl_config_raw = deps
                        .querier
                        .query_wasm_raw(&pair_addr, b"config")?
                        .ok_or_else(|| StdError::generic_err("PCL config not found"))?;
                    let pcl_config = convert_pcl_config(pcl_config_raw)?;
                    let precisions = balances
                        .iter()
                        .map(|asset| {
                            let prec = Precisions::PRECISIONS
                                .query(&deps.querier, pair_addr.clone(), asset.info.to_string())?
                                .or_else(|| {
                                    asset
                                        .info
                                        .decimals(&deps.querier, &pcl_config.factory_addr)
                                        .ok()
                                })
                                .ok_or_else(|| {
                                    StdError::generic_err(format!(
                                        "Asset {} precision not found",
                                        &asset.info
                                    ))
                                })?;
                            Ok((asset.info.to_string(), prec))
                        })
                        .collect::<StdResult<HashMap<_, _>>>()?;
                    let dec_balances = balances
                        .into_iter()
                        .map(|asset| {
                            asset
                                .to_decimal_asset(*precisions.get(&asset.info.to_string()).unwrap())
                                .map_err(Into::into)
                        })
                        .collect::<StdResult<Vec<_>>>()?;
                    let total_share = query_supply(&deps.querier, &pair_info.liquidity_token)?;
                    pcl_provide_simulation(
                        env,
                        dec_balances,
                        assets,
                        total_share,
                        pcl_config,
                        precisions,
                    )
                    .map_err(|err| StdError::generic_err(err.to_string()))
                    .and_then(|res| to_json_binary(&res))
                }
                PairType::Custom(_) => unimplemented!("not implemented yet"),
            }
        }
        _ => Err(StdError::generic_err("Invalid simulate message")),
    }
}

fn simulate_withdraw(deps: Deps, pair_addr: String, lp_tokens: Uint128) -> StdResult<Binary> {
    let pair_addr = deps.api.addr_validate(&pair_addr)?;
    let assets: Vec<Asset> = deps
        .querier
        .query_wasm_smart(pair_addr, &PairQueryMsg::Share { amount: lp_tokens })?;

    if assets.len() != 2 {
        return Err(StdError::generic_err(format!(
            "{}",
            ContractError::WrongPoolLength {}
        )));
    }

    to_json_binary(&assets)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::{Addr, Decimal};

    use astroport::asset::{native_asset_info, token_asset_info, AssetInfoExt};
    use astroport::liquidity_manager::{Cw20HookMsg, ExecuteMsg};
    use astroport::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg};

    use super::*;

    #[test]
    fn generate_query_msg_examples() {
        let provide_query_msg = QueryMsg::SimulateProvide {
            pair_addr: "wasm1...addr".to_string(),
            pair_msg: PairExecuteMsg::ProvideLiquidity {
                assets: vec![
                    native_asset_info("uusd".to_string()).with_balance(100000u128),
                    token_asset_info(Addr::unchecked("wasm1...cw20address".to_string()))
                        .with_balance(100000u128),
                ],
                slippage_tolerance: Some(Decimal::from_str("0.02").unwrap()),
                auto_stake: Some(true),
                receiver: Some("wasm1...addr".to_string()),
            },
        };

        println!(
            "Provide example query msg: {}",
            serde_json::to_string_pretty(&provide_query_msg).unwrap()
        );

        let withdraw_query_msg = QueryMsg::SimulateWithdraw {
            pair_addr: "wasm1...addr".to_string(),
            lp_tokens: 1000u16.into(),
        };

        println!(
            "Withdraw example query msg: {}",
            serde_json::to_string_pretty(&withdraw_query_msg).unwrap()
        );
    }

    #[test]
    fn generate_execute_msg_examples() {
        let provide_msg = ExecuteMsg::ProvideLiquidity {
            pair_addr: "wasm1...pair address".to_string(),
            pair_msg: PairExecuteMsg::ProvideLiquidity {
                assets: vec![
                    native_asset_info("uusd".to_string()).with_balance(100000u128),
                    token_asset_info(Addr::unchecked("wasm1...cw20address".to_string()))
                        .with_balance(100000u128),
                ],
                slippage_tolerance: Some(Decimal::from_str("0.02").unwrap()),
                auto_stake: Some(true),
                receiver: Some("wasm1...addr".to_string()),
            },
            min_lp_to_receive: Some(100000u128.into()),
        };

        println!(
            "Provide example execute msg: {}",
            serde_json::to_string_pretty(&provide_msg).unwrap()
        );

        let cw20hook_msg = Cw20HookMsg::WithdrawLiquidity {
            pair_msg: PairCw20HookMsg::WithdrawLiquidity { assets: vec![] },
            min_assets_to_receive: vec![
                native_asset_info("uusd".to_string()).with_balance(100000u128),
                token_asset_info(Addr::unchecked("wasm1...cw20address".to_string()))
                    .with_balance(100000u128),
            ],
        };

        let withdraw_msg = cw20::Cw20ExecuteMsg::Send {
            contract: "wasm1...LP token address".to_string(),
            amount: 1000u128.into(),
            msg: to_json_binary(&cw20hook_msg).unwrap(),
        };

        println!(
            "Withdraw example execute msg: {}",
            serde_json::to_string_pretty(&withdraw_msg).unwrap()
        );

        println!(
            "Where base64-encoded cw20 hook msg is: {}",
            serde_json::to_string_pretty(&cw20hook_msg).unwrap()
        );
    }
}
