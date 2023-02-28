use astroport::{
    asset::{AssetInfo, PairInfo},
    querier::query_token_precision,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, QuerierWrapper, StdResult, Uint128};
use cw_storage_plus::Item;

use crate::state::{store_precisions, Config, CONFIG};

/// This structure stores the main stableswap pair parameters.
#[cw_serde]
pub struct ConfigV100 {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract update the asset cumulative prices
    pub block_time_last: u64,
    /// The last cumulative price for asset 0
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price for asset 1
    pub price1_cumulative_last: Uint128,
    /// This is the current amplification used in the pool
    pub init_amp: u64,
    /// This is the start time when amplification starts to scale up or down
    pub init_amp_time: u64,
    /// This is the target amplification to reach at `next_amp_time`
    pub next_amp: u64,
    /// This is the timestamp when the current pool amplification should be `next_amp`
    pub next_amp_time: u64,
}

pub const CONFIG_V100: Item<ConfigV100> = Item::new("config");

/// Validates array of assets. If asset is native coin then this function checks whether
/// it has been registered in registry or not.
pub(crate) fn is_native_registered(
    querier: &QuerierWrapper,
    asset_infos: &[AssetInfo],
    factory_addr: &Addr,
) -> StdResult<()> {
    for asset_info in asset_infos {
        query_token_precision(querier, asset_info, factory_addr)?;
    }

    Ok(())
}

pub fn migrate_config_to_v210(mut deps: DepsMut) -> StdResult<Config> {
    let cfg_v100 = CONFIG_V100.load(deps.storage)?;

    is_native_registered(
        &deps.querier,
        &cfg_v100.pair_info.asset_infos,
        &cfg_v100.factory_addr,
    )?;

    let cumulative_prices = vec![
        (
            cfg_v100.pair_info.asset_infos[0].clone(),
            cfg_v100.pair_info.asset_infos[1].clone(),
            cfg_v100.price0_cumulative_last,
        ),
        (
            cfg_v100.pair_info.asset_infos[1].clone(),
            cfg_v100.pair_info.asset_infos[0].clone(),
            cfg_v100.price1_cumulative_last,
        ),
    ];
    let greatest_precision = store_precisions(
        deps.branch(),
        &cfg_v100.pair_info.asset_infos,
        &cfg_v100.factory_addr,
    )?;

    let cfg = Config {
        owner: None,
        pair_info: cfg_v100.pair_info,
        factory_addr: cfg_v100.factory_addr,
        block_time_last: cfg_v100.block_time_last,
        init_amp: cfg_v100.next_amp,
        init_amp_time: cfg_v100.init_amp_time,
        next_amp: cfg_v100.next_amp,
        next_amp_time: cfg_v100.next_amp_time,
        greatest_precision,
        cumulative_prices,
    };

    CONFIG.save(deps.storage, &cfg)?;

    Ok(cfg)
}
