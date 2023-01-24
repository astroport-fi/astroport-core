use crate::state::{Config, CONFIG};
use astroport::asset::token_asset_info;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, StdResult};
use cw_storage_plus::Item;

pub(crate) fn migrate_from_v100(deps: DepsMut) -> StdResult<()> {
    #[cw_serde]
    struct OldConfig {
        pub owner: Addr,
        pub token_addr: Addr,
    }
    let cfg_v100: OldConfig = Item::new("config").load(deps.storage)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: cfg_v100.owner,
            vesting_token: token_asset_info(cfg_v100.token_addr),
        },
    )
}
