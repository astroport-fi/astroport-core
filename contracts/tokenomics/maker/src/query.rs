use crate::error::ContractError;
use crate::state::{CONFIG, ROUTES, SEIZE_CONFIG};
use crate::utils::{asset_info_key, from_key_to_asset_info, RoutesBuilder};
use astroport::asset::{determine_asset_info, Asset, AssetInfoExt};
use astroport::maker::{PoolRoute, QueryMsg, RouteStep, DEFAULT_PAGINATION_LIMIT};
use astroport::pair;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Order, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let result = match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Route { asset_in } => to_json_binary(&get_route(deps, &asset_in)?),
        QueryMsg::Routes { start_after, limit } => {
            to_json_binary(&query_routes(deps, start_after, limit)?)
        }
        QueryMsg::EstimateSwap { asset_in } => to_json_binary(&estimate_swap(deps, asset_in)?),
        QueryMsg::QuerySeizeConfig {} => to_json_binary(&SEIZE_CONFIG.load(deps.storage)?),
    }?;

    Ok(result)
}

pub fn get_route(deps: Deps, denom_in: &str) -> Result<Vec<RouteStep>, ContractError> {
    let asset_in = determine_asset_info(denom_in, deps.api)?;
    let config = CONFIG.load(deps.storage)?;

    RoutesBuilder::default().build_routes(deps.storage, &asset_in, &config.astro_denom)
}

pub fn query_routes(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<PoolRoute>> {
    let limit = limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize;
    let start_after = start_after
        .map(|asset| {
            determine_asset_info(&asset, deps.api).map(|asset_info| asset_info_key(&asset_info))
        })
        .transpose()?;

    ROUTES
        .range(
            deps.storage,
            start_after.as_deref().map(Bound::exclusive),
            None,
            Order::Ascending,
        )
        .map(|item| {
            item.and_then(|(asset_in_key, route_step)| {
                Ok(PoolRoute {
                    asset_in: from_key_to_asset_info(asset_in_key)?,
                    asset_out: route_step.asset_out,
                    pool_addr: route_step.pool_addr.to_string(),
                })
            })
        })
        .take(limit)
        .collect()
}

pub fn estimate_swap(deps: Deps, mut asset_in: Asset) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut routes_builder = RoutesBuilder::default();
    let routes = routes_builder.build_routes(deps.storage, &asset_in.info, &config.astro_denom)?;

    for step in routes {
        let sim_out_amount: pair::SimulationResponse = deps.querier.query_wasm_smart(
            &step.pool_addr,
            &pair::QueryMsg::Simulation {
                offer_asset: asset_in,
                ask_asset_info: Some(step.asset_out.clone()),
            },
        )?;

        asset_in = step.asset_out.with_balance(sim_out_amount.return_amount);
    }

    Ok(asset_in.amount)
}
