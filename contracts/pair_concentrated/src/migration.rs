use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_json, to_json_binary, Addr, Env, StdError, Storage, Uint128};
use cw_storage_plus::Item;

use astroport::asset::PairInfo;
use astroport::pair::FeeShareConfig;
use astroport_pcl_common::state::{Config, PoolParams, PoolState};

use crate::state::CONFIG;

pub(crate) fn migrate_config(storage: &mut dyn Storage) -> Result<(), StdError> {
    let old_config = astroport_pair_concentrated_v1::state::CONFIG.load(storage)?;
    let new_config = Config {
        pair_info: from_json(to_json_binary(&old_config.pair_info)?)?,
        factory_addr: old_config.factory_addr,
        block_time_last: old_config.block_time_last,
        cumulative_prices: from_json(to_json_binary(&old_config.cumulative_prices)?)?,
        pool_params: from_json(to_json_binary(&old_config.pool_params)?)?,
        pool_state: from_json(to_json_binary(&old_config.pool_state)?)?,
        owner: old_config.owner,
        track_asset_balances: old_config.track_asset_balances,
        fee_share: None,
    };

    CONFIG.save(storage, &new_config)?;

    Ok(())
}

pub(crate) fn migrate_config_v2(storage: &mut dyn Storage, env: &Env) -> Result<(), StdError> {
    #[cw_serde]
    struct OldConfig {
        pub pair_info: PairInfo,
        pub factory_addr: Addr,
        pub pool_params: PoolParams,
        pub pool_state: PoolState,
        pub owner: Option<Addr>,
        pub track_asset_balances: bool,
        pub fee_share: Option<FeeShareConfig>,
    }

    let old_config = Item::<OldConfig>::new("config").load(storage)?;
    // Initializing cumulative prices
    let cumulative_prices = vec![
        (
            old_config.pair_info.asset_infos[0].clone(),
            old_config.pair_info.asset_infos[1].clone(),
            Uint128::zero(),
        ),
        (
            old_config.pair_info.asset_infos[1].clone(),
            old_config.pair_info.asset_infos[0].clone(),
            Uint128::zero(),
        ),
    ];
    let new_config = Config {
        pair_info: old_config.pair_info,
        factory_addr: old_config.factory_addr,
        block_time_last: env.block.time.seconds(),
        cumulative_prices,
        pool_params: old_config.pool_params,
        pool_state: old_config.pool_state,
        owner: old_config.owner,
        track_asset_balances: old_config.track_asset_balances,
        fee_share: old_config.fee_share,
    };

    CONFIG.save(storage, &new_config)?;

    Ok(())
}
