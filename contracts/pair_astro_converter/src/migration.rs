use cosmwasm_std::{Addr, QuerierWrapper, StdResult, Storage};
use cw_storage_plus::Item;
use serde::{Deserialize, Serialize};

use astroport::asset::PairInfo;
use astroport::astro_converter;

use crate::state::Config;
use crate::state::CONFIG;

/// This structure partially captures config of the XYK pair contract.
/// We don't use cw_serde macro intentionally to allow unknown fields in the config.
/// Thus migration is compatible with any XYK pair version.
#[derive(Serialize, Deserialize)]
struct PartialConfig {
    pub pair_info: PairInfo,
    pub factory_addr: Addr,
}

pub fn migrate_config(storage: &mut dyn Storage, converter_contract: Addr) -> StdResult<Config> {
    let partial_config: PartialConfig = Item::new("config").load(storage)?;
    let new_config = Config {
        pair_info: partial_config.pair_info,
        factory_addr: partial_config.factory_addr,
        converter_contract,
    };

    CONFIG.save(storage, &new_config)?;

    Ok(new_config)
}

pub fn sanity_checks(querier: QuerierWrapper, config: &Config) -> StdResult<()> {
    let converter_config =
        querier.query_wasm_smart::<astro_converter::Config>(&config.converter_contract)?;

    Ok(())
}
