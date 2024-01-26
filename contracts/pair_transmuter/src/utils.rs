use cosmwasm_std::{ensure, Api, Decimal, Deps, QuerierWrapper, StdResult, Uint128};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::querier::query_supply;

use crate::error::ContractError;
use crate::state::{Config, CONFIG};

/// Helper function to check if the given asset infos are valid.
pub fn check_asset_infos(api: &dyn Api, asset_infos: &[AssetInfo]) -> Result<(), ContractError> {
    if !asset_infos.iter().all_unique() {
        return Err(ContractError::DoublingAssets {});
    }

    asset_infos.iter().try_for_each(|asset_info| {
        if !asset_info.is_native_token() {
            Err(ContractError::Cw20TokenNotSupported {})
        } else {
            Ok(asset_info.check(api)?)
        }
    })
}

/// Helper function to check that the assets in a given array are valid.
pub fn check_assets(api: &dyn Api, assets: &[Asset]) -> Result<(), ContractError> {
    let asset_infos = assets.iter().map(|asset| asset.info.clone()).collect_vec();
    check_asset_infos(api, &asset_infos)
}

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub fn pool_info(querier: QuerierWrapper, config: &Config) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;
    let total_share = query_supply(&querier, &config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

pub fn get_share_in_assets(
    pools: &[Asset],
    amount: Uint128,
    total_share: Uint128,
) -> Result<Vec<Asset>, ContractError> {
    let share_ratio = Decimal::checked_from_ratio(amount, total_share)?;

    let assets = pools
        .iter()
        .map(|asset| asset.info.with_balance(asset.amount * share_ratio))
        .collect();

    Ok(assets)
}

pub fn assert_and_swap(
    deps: Deps,
    offer_asset: &Asset,
    ask_asset_info: Option<AssetInfo>,
) -> Result<(Asset, AssetInfo), ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        config
            .pair_info
            .asset_infos
            .iter()
            .contains(&offer_asset.info),
        ContractError::InvalidAsset(offer_asset.info.to_string())
    );

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let ask_asset_info = if config.pair_info.asset_infos.len() > 2 {
        ask_asset_info.ok_or(ContractError::AskAssetMustBeSet {})?
    } else {
        config
            .pair_info
            .asset_infos
            .iter()
            .find(|&asset_info| asset_info != &offer_asset.info)
            .cloned()
            .unwrap()
    };

    let ask_pool = pools
        .iter()
        .find(|asset| asset.info == ask_asset_info)
        .ok_or_else(|| ContractError::InvalidAsset(ask_asset_info.to_string()))?;

    if ask_pool.amount >= offer_asset.amount {
        Ok((
            ask_pool.info.with_balance(offer_asset.amount),
            ask_asset_info,
        ))
    } else {
        Err(ContractError::InsufficientPoolBalance {
            asset: ask_asset_info.to_string(),
            want: offer_asset.amount,
            available: ask_pool.amount,
        })
    }
}
