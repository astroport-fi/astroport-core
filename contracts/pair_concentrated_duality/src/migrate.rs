use cosmwasm_std::{
    attr, ensure, ensure_eq, entry_point, DepsMut, Env, Response, StdError, StdResult,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Item;

use astroport::factory::PairType;
use astroport::pair_concentrated_duality::MigrateMsg;
use astroport_pcl_common::state::Config;

use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::orderbook::state::OrderbookState;
use crate::state::CONFIG;

const MIGRATE_FROM: &str = "astroport-pair-concentrated";
const VERSION_REQ: &str = ">=4.0.0, <5.0.0";

fn from_semver(err: semver::Error) -> StdError {
    StdError::generic_err(format!("Semver: {err}"))
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let mut attrs = vec![];

    let stored_info = get_contract_version(deps.storage)?;
    match msg {
        MigrateMsg::MigrateToOrderbook { orderbook_config } => {
            let version_req = semver::VersionReq::parse(VERSION_REQ).map_err(from_semver)?;
            let stored_ver = semver::Version::parse(&stored_info.version).map_err(from_semver)?;
            ensure!(
                stored_info.contract == MIGRATE_FROM && version_req.matches(&stored_ver),
                StdError::generic_err(format!(
                    "Can migrate only from {MIGRATE_FROM} {VERSION_REQ}"
                ))
            );

            let mut config: Config = Item::new("config").load(deps.storage)?;
            let ob_state = OrderbookState::new(deps.api, orderbook_config)?;
            config.pair_info.pair_type =
                PairType::Custom("concentrated_duality_orderbook".to_string());
            CONFIG.save(deps.storage, &config)?;

            attrs.push(attr("action", "migrate_to_orderbook"));

            ob_state.save(deps.storage)
        }
        MigrateMsg::Migrate {} => {
            ensure_eq!(
                stored_info.contract,
                CONTRACT_NAME,
                StdError::generic_err(format!("This endpoint is allowed only for {CONTRACT_NAME}"))
            );

            Err(StdError::generic_err("Not yet implemented".to_string()))
        }
    }?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    attrs.extend([
        attr("previous_contract_name", stored_info.contract),
        attr("previous_contract_version", stored_info.version),
        attr("new_contract_name", CONTRACT_NAME),
        attr("new_contract_version", CONTRACT_VERSION),
    ]);
    Ok(Response::default().add_attributes(attrs))
}
