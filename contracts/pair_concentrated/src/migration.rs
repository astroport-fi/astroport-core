use cosmwasm_std::{from_binary, to_binary, StdError, Storage};

use astroport_pcl_common::state::Config;

use crate::state::CONFIG;

pub(crate) fn migrate_config(storage: &mut dyn Storage) -> Result<(), StdError> {
    let old_config = astroport_pair_concentrated_v1::state::CONFIG.load(storage)?;
    let new_config = Config {
        pair_info: from_binary(&to_binary(&old_config.pair_info)?)?,
        factory_addr: old_config.factory_addr,
        pool_params: from_binary(&to_binary(&old_config.pool_params)?)?,
        pool_state: from_binary(&to_binary(&old_config.pool_state)?)?,
        owner: old_config.owner,
        track_asset_balances: old_config.track_asset_balances,
        fee_share: None,
    };

    CONFIG.save(storage, &new_config)?;

    Ok(())
}
