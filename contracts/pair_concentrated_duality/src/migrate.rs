use cosmwasm_std::{entry_point, DepsMut, Empty, Env, Response, StdResult};

// const MIGRATE_FROM: &str = "astroport-pair-concentrated";
// const MIGRATION_VERSION: &str = "4.0.1";

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    todo!("add seamless migration from PCL to PCL with Duality integration")

    // let mut attrs = vec![];
    //
    // let contract_info = CONTRACT.load(deps.storage)?;
    // match msg {
    //     MigrateMsg::MigrateToOrderbook { params } => {
    //         if contract_info.contract != MIGRATE_FROM || contract_info.version != MIGRATION_VERSION
    //         {
    //             return Err(StdError::generic_err(format!(
    //                 "Can't migrate from {} {}",
    //                 contract_info.contract, contract_info.version
    //             )));
    //         }
    //
    //         let mut config: Config = Item::new("config").load(deps.storage)?;
    //         let base_precision =
    //             config.pair_info.asset_infos[0].decimals(&deps.querier, &config.factory_addr)?;
    //         let ob_state = OrderbookState::new(
    //             deps.querier,
    //             &env,
    //             &params.market_id,
    //             params.orders_number,
    //             params.min_trades_to_avg,
    //             &config.pair_info.asset_infos,
    //             base_precision,
    //         )?;
    //         config.pair_info.pair_type = PairType::Custom("concentrated_inj_orderbook".to_string());
    //         CONFIG.save(deps.storage, &config)?;
    //         ob_state.save(deps.storage)?;
    //
    //         attrs.push(attr("action", "migrate_to_orderbook"));
    //         attrs.push(attr("subaccount_id", ob_state.subaccount.to_string()))
    //     }
    //     MigrateMsg::Migrate {} => {
    //         let contract_info = cw2::get_contract_version(deps.storage)?;
    //         match contract_info.contract.as_str() {
    //             CONTRACT_NAME => match contract_info.version.as_str() {
    //                 "2.0.3" | "2.0.4" => {}
    //                 _ => {
    //                     return Err(StdError::generic_err(format!(
    //                         "Can't migrate from {} {}",
    //                         contract_info.contract, contract_info.version
    //                     )));
    //                 }
    //             },
    //             _ => {
    //                 return Err(StdError::generic_err(format!(
    //                     "Can't migrate from {} {}",
    //                     contract_info.contract, contract_info.version
    //                 )));
    //             }
    //         }
    //     }
    // }
    //
    // set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    //
    // attrs.extend([
    //     attr("previous_contract_name", contract_info.contract),
    //     attr("previous_contract_version", contract_info.version),
    //     attr("new_contract_name", CONTRACT_NAME),
    //     attr("new_contract_version", CONTRACT_VERSION),
    // ]);
    // Ok(Response::default().add_attributes(attrs))
}
