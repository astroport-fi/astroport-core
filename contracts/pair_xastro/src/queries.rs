use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdResult, Storage, Uint128};

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::pair::{
    ConfigResponse, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
};
use astroport::querier::query_factory_config;

use crate::contract::{predict_stake, predict_unstake};
use crate::error::ContractError;
use crate::state::{Config, CONFIG};

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?.pair_info)?),
        QueryMsg::Pool {} => Ok(to_json_binary(&query_pool(deps.storage)?)?),
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        QueryMsg::Share { .. } => Ok(to_json_binary(&empty_share(deps.storage)?)?),
        QueryMsg::Simulation { offer_asset, .. } => {
            let config = CONFIG.load(deps.storage)?;
            let return_amount = match &offer_asset.info {
                AssetInfo::NativeToken { denom } if denom == &config.astro_denom => {
                    predict_stake(deps.querier, &config, offer_asset.amount)
                }
                AssetInfo::NativeToken { denom } if denom == &config.xastro_denom => {
                    predict_unstake(deps.querier, &config, offer_asset.amount)
                }
                _ => Err(ContractError::InvalidAsset(offer_asset.info.to_string())),
            }?;

            Ok(to_json_binary(&SimulationResponse {
                return_amount,
                spread_amount: Uint128::zero(),
                commission_amount: Uint128::zero(),
            })?)
        }
        QueryMsg::ReverseSimulation { ask_asset, .. } => {
            let config = CONFIG.load(deps.storage)?;

            let offer_amount = match &ask_asset.info {
                AssetInfo::NativeToken { denom } if denom == &config.astro_denom => {
                    predict_stake(deps.querier, &config, ask_asset.amount)
                }
                AssetInfo::NativeToken { denom } if denom == &config.xastro_denom => {
                    predict_unstake(deps.querier, &config, ask_asset.amount)
                }
                _ => Err(ContractError::InvalidAsset(ask_asset.info.to_string())),
            }?;

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
        tracker_addr: None,
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
