use cosmwasm_std::{
    ensure, entry_point, to_json_binary, Binary, Deps, Env, StdResult, Storage, Uint128,
};

use astroport::asset::{Asset, AssetInfoExt};
use astroport::pair::{
    ConfigResponse, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
};
use astroport::querier::query_factory_config;

use crate::error::ContractError;
use crate::state::{Config, CONFIG};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?.pair_info)?),
        QueryMsg::Pool {} => Ok(to_json_binary(&query_pool(deps.storage)?)?),
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        QueryMsg::Share { .. } => Ok(to_json_binary(&empty_share(deps.storage)?)?),
        QueryMsg::Simulation { offer_asset, .. } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                offer_asset.info == config.from,
                ContractError::AssetMismatch {
                    old: config.from.to_string(),
                    new: config.to.to_string()
                }
            );

            Ok(to_json_binary(&SimulationResponse {
                return_amount: offer_asset.amount,
                spread_amount: Uint128::zero(),
                commission_amount: Uint128::zero(),
            })?)
        }
        QueryMsg::ReverseSimulation { ask_asset, .. } => {
            let config = CONFIG.load(deps.storage)?;

            // Assert ask_asset belongs to the pair
            let in_pair = config.pair_info.asset_infos.contains(&ask_asset.info);

            ensure!(
                in_pair && ask_asset.info != config.from,
                ContractError::AssetMismatch {
                    old: config.from.to_string(),
                    new: config.to.to_string()
                }
            );

            Ok(to_json_binary(&ReverseSimulationResponse {
                offer_amount: ask_asset.amount,
                spread_amount: Uint128::zero(),
                commission_amount: Uint128::zero(),
            })?)
        }
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
pub fn query_pool(storage: &dyn Storage) -> StdResult<PoolResponse> {
    let resp = PoolResponse {
        assets: empty_share(storage)?,
        total_share: Uint128::zero(),
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

pub fn empty_share(storage: &dyn Storage) -> StdResult<Vec<Asset>> {
    let share = CONFIG
        .load(storage)?
        .pair_info
        .asset_infos
        .iter()
        .map(|asset_info| asset_info.with_balance(0u128))
        .collect();

    Ok(share)
}
