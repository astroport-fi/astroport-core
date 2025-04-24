use std::collections::HashMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, ensure, ensure_eq, entry_point, Addr, Decimal, DepsMut, Env, Response, StdError,
    StdResult, Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Item;

use astroport::asset::Asset;
use astroport::factory::PairType;
use astroport::pair_concentrated_duality::MigrateMsg;
use astroport_pcl_common::state::Config;

use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::orderbook::state::{OrderState, OrderbookState};
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

            match stored_info.version.as_str() {
                "4.1.1" => {
                    #[cw_serde]
                    pub struct OldOrderbookState {
                        pub executor: Option<Addr>,
                        pub orders_number: u8,
                        pub min_asset_0_order_size: Uint128,
                        pub min_asset_1_order_size: Uint128,
                        pub liquidity_percent: Decimal,
                        pub orders: Vec<String>,
                        pub enabled: bool,
                        pub pre_reply_contract_balances: Vec<Asset>,
                        pub delayed_trade: Option<OrderState>,
                        pub avg_price_adjustment: Decimal,
                        pub last_order_sizes: HashMap<String, Uint128>,
                    }
                    let old_ob_state: OldOrderbookState =
                        Item::new("orderbook_config").load(deps.storage)?;

                    ensure!(
                        old_ob_state.delayed_trade.is_none(),
                        StdError::generic_err(
                            "Delayed trade must be none. Sync orderbook and migrate after"
                        )
                    );

                    ensure!(
                        !old_ob_state.enabled,
                        StdError::generic_err("Orderbook must be disabled")
                    );

                    let ob_state = OrderbookState {
                        executor: old_ob_state.executor,
                        orders_number: old_ob_state.orders_number,
                        min_asset_0_order_size: old_ob_state.min_asset_0_order_size,
                        min_asset_1_order_size: old_ob_state.min_asset_1_order_size,
                        liquidity_percent: old_ob_state.liquidity_percent,
                        orders: vec![],
                        enabled: false,
                        pre_reply_balances: vec![],
                        delayed_trade: None,
                        avg_price_adjustment: old_ob_state.avg_price_adjustment,
                        orders_state: Default::default(),
                        old_orders_state: Default::default(),
                    };

                    Item::new("orderbook_config").save(deps.storage, &ob_state)?;

                    Ok(())
                }
                _ => Err(StdError::generic_err("Invalid contract version")),
            }
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
