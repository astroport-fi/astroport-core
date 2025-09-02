use astroport::asset::determine_asset_info;
use astroport::maker::{PoolRoute, QueryMsg, RouteStep, DEFAULT_PAGINATION_LIMIT};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Coin, Deps, Env, Order, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::state::{CONFIG, ROUTES, SEIZE_CONFIG};
use crate::utils::RoutesBuilder;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let result = match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Route {
            asset_in,
            asset_out,
        } => to_json_binary(&query_route(deps, &asset_in, &asset_out)?),
        QueryMsg::Routes { start_after, limit } => {
            to_json_binary(&query_routes(deps.storage, start_after, limit)?)
        }
        QueryMsg::EstimateExactInSwap { asset_in } => {
            to_json_binary(&estimate_exact_swap_in(deps, asset_in)?)
        }
        QueryMsg::QuerySeizeConfig {} => to_json_binary(&SEIZE_CONFIG.load(deps.storage)?),
    }?;

    Ok(result)
}

pub fn query_route(
    deps: Deps,
    denom_in: &str,
    denom_out: &str,
) -> Result<Vec<RouteStep>, ContractError> {
    let asset_in = determine_asset_info(denom_in, deps.api)?;
    let asset_out = determine_asset_info(denom_out, deps.api)?;

    RoutesBuilder::default().build_routes(deps.storage, &asset_in, &asset_out)
}

pub fn query_routes(
    storage: &dyn Storage,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<PoolRoute>> {
    let limit = limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize;

    ROUTES
        .range(
            storage,
            start_after.as_deref().map(Bound::exclusive),
            None,
            Order::Ascending,
        )
        .map(|item| {
            item.map(|(denom_in, route_step)| PoolRoute {
                denom_in,
                denom_out: route_step.asset_out,
                pool_id: route_step.pool_addr,
            })
        })
        .take(limit)
        .collect()
}

pub fn estimate_exact_swap_in(deps: Deps, coin_in: Coin) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut routes_builder = RoutesBuilder::default();
    let built_routes =
        routes_builder.build_routes(deps.storage, &coin_in.denom, &config.astro_denom)?;

    query_out_amount(deps.querier, &coin_in, &built_routes.routes)
}
