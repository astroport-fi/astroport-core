use cosmwasm_std::{entry_point, to_json_binary, Binary, Deps, Env, StdResult, Uint128};

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::pair::{
    ConfigResponse, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
};
use astroport::querier::query_factory_config;

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use crate::utils::{assert_and_swap, get_share_in_assets, pool_info};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?.pair_info)?),
        QueryMsg::Pool {} => Ok(to_json_binary(&query_pool(deps)?)?),
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        QueryMsg::Share { amount } => Ok(to_json_binary(&query_share(deps, amount)?)?),
        QueryMsg::Simulation {
            offer_asset,
            ask_asset_info,
        } => {
            let return_asset = assert_and_swap(deps, &offer_asset, ask_asset_info)?;

            Ok(to_json_binary(&SimulationResponse {
                return_amount: return_asset.amount,
                spread_amount: Uint128::zero(),
                commission_amount: Uint128::zero(),
            })?)
        }
        QueryMsg::ReverseSimulation {
            offer_asset_info,
            ask_asset,
        } => {
            let offer_amount = reverse_swap(deps, offer_asset_info, ask_asset)?;

            Ok(to_json_binary(&ReverseSimulationResponse {
                offer_amount,
                spread_amount: Uint128::zero(),
                commission_amount: Uint128::zero(),
            })?)
        }
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

/// Returns the pair contract configuration in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    Ok(ConfigResponse {
        block_time_last: 0,
        params: None,
        owner: factory_config.owner,
        factory_addr: config.factory_addr,
    })
}

/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **amount** is the amount of LP tokens for which we calculate associated amounts of assets.
pub fn query_share(deps: Deps, amount: Uint128) -> Result<Vec<Asset>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(deps.querier, &config)?;

    get_share_in_assets(&pools, amount, total_share)
}

/// Returns the amount of offer_asset required to swap for a specific amount of ask_asset.
/// offer_asset_info must be set if the pair contract contains more than 2 assets.
pub fn reverse_swap(
    deps: Deps,
    offer_asset_info: Option<AssetInfo>,
    ask_asset: Asset,
) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let offer_asset_info = if let Some(offer_asset_info) = offer_asset_info {
        if !config.pair_info.asset_infos.contains(&offer_asset_info) {
            return Err(ContractError::InvalidAsset(offer_asset_info.to_string()));
        } else {
            offer_asset_info
        }
    } else {
        config
            .pair_info
            .asset_infos
            .iter()
            .find(|&asset_info| asset_info != &ask_asset.info)
            .cloned()
            .unwrap()
    };

    let ask_pool = pools
        .iter()
        .find(|asset| asset.info == ask_asset.info)
        .ok_or_else(|| ContractError::InvalidAsset(ask_asset.info.to_string()))?;

    let ask_asset = config.normalize(&ask_asset)?;

    if ask_pool.amount >= ask_asset.amount {
        let offer_asset = offer_asset_info.with_balance(ask_asset.amount);
        config.denormalize(&offer_asset).map(|asset| asset.amount)
    } else {
        Err(ContractError::InsufficientPoolBalance {
            asset: ask_asset.info.to_string(),
            want: ask_asset.amount,
            available: ask_pool.amount,
        })
    }
}
