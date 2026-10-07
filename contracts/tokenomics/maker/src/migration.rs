use cosmwasm_std::{Addr, Decimal, DepsMut, Env, Response};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};

use astroport::asset::AssetInfo;

use crate::contract::{set_routes, validate_config, CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use crate::msg::{Config, MigrateMsg};
use crate::state::CONFIG;
use crate::utils::{cooldown_or_none, validate_collectors};

/// The parts of a Maker 1.7.0 config that carry over. Its other fields (governance, second
/// receiver, dev fund, pre-upgrade rewards) are replaced by legs, so they're not read; serde
/// ignores them.
#[derive(Serialize, Deserialize)]
struct ConfigV170 {
    owner: Addr,
    max_spread: Decimal,
    collect_cooldown: Option<u64>,
}

pub(crate) fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME || version.version != "1.7.0" {
        return Err(ContractError::MigrationError {});
    }

    let old: ConfigV170 = Item::new("config").load(deps.storage)?;

    let config = Config {
        owner: old.owner,
        router: deps.api.addr_validate(&msg.router)?,
        base_asset: msg.base_asset,
        max_spread: msg.max_spread.unwrap_or(old.max_spread),
        collect_cooldown: match msg.collect_cooldown {
            None => old.collect_cooldown,
            set => cooldown_or_none(set),
        },
        collectors: validate_collectors(deps.api, &msg.collectors)?,
        legs: msg.legs,
    };
    validate_config(deps.as_ref(), &env, &config)?;
    CONFIG.save(deps.storage, &config)?;

    // 1.x bridges (fee token -> next asset toward ASTRO) are replaced by full routes to the base
    // asset; the seize config, last collect time and any ownership proposal are kept as they are
    let bridges: Map<String, AssetInfo> = Map::new("bridges");
    bridges.clear(deps.storage);
    set_routes(
        deps.storage,
        deps.api,
        &config.base_asset,
        msg.routes,
        vec![],
    )?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", version.contract)
        .add_attribute("previous_contract_version", version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
