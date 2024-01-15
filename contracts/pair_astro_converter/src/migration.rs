use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, StdError, StdResult, Storage};
use cw_storage_plus::Item;
use serde::{Deserialize, Serialize};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::astro_converter;

use crate::state::Config;
use crate::state::CONFIG;

#[cw_serde]
pub struct MigrateMsg {
    pub converter_contract: String,
}

/// This structure partially captures config of the XYK pair contract.
/// We don't use cw_serde macro intentionally to allow unknown fields in the config.
/// Thus migration is compatible with any XYK pair version.
#[derive(Serialize, Deserialize)]
struct PartialConfig {
    pub pair_info: PairInfo,
    pub factory_addr: Addr,
}

pub fn migrate_config(
    storage: &mut dyn Storage,
    converter_contract: Addr,
    converter_config: &astro_converter::Config,
) -> StdResult<Config> {
    let partial_config: PartialConfig = Item::new("config").load(storage)?;
    let new_config = Config {
        pair_info: partial_config.pair_info,
        factory_addr: partial_config.factory_addr,
        converter_contract,
        from: converter_config.old_astro_asset_info.clone(),
        to: AssetInfo::native(&converter_config.new_astro_denom),
    };

    CONFIG.save(storage, &new_config)?;

    Ok(new_config)
}

pub fn sanity_checks(config: &Config, converter_config: &astro_converter::Config) -> StdResult<()> {
    ensure!(
        config.pair_info.asset_infos.len() == 2,
        StdError::generic_err("Only 2 assets are supported")
    );

    ensure!(
        config
            .pair_info
            .asset_infos
            .contains(&converter_config.old_astro_asset_info),
        StdError::generic_err("Pair doesn't have old ASTRO specified in the converter contract")
    );

    ensure!(
        config
            .pair_info
            .asset_infos
            .contains(&AssetInfo::native(&converter_config.new_astro_denom)),
        StdError::generic_err(
            "Pair doesn't have new ASTRO denom specified in the converter contract"
        )
    );

    Ok(())
}
