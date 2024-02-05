use cosmwasm_std::{attr, entry_point, from_json, DepsMut, Env, Response, StdError, StdResult};
use cw2::{set_contract_version, CONTRACT};
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper};

use astroport::pair_concentrated_inj::{MigrateMsg, OrderbookConfig};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::orderbook::state::OrderbookState;

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    deps: DepsMut<InjectiveQueryWrapper>,
    _env: Env,
    msg: MigrateMsg,
) -> StdResult<Response<InjectiveMsgWrapper>> {
    let mut attrs = vec![];

    let contract_info = CONTRACT.load(deps.storage)?;
    match msg {
        MigrateMsg::MigrateWithParams(data) => {
            let contract_info = cw2::get_contract_version(deps.storage)?;
            match contract_info.contract.as_str() {
                "2.2.2" => {
                    let OrderbookConfig {
                        liquidity_percent,
                        min_base_order_size,
                        min_quote_order_size,
                        orders_number,
                        ..
                    } = from_json(data)?;

                    let mut ob_state = OrderbookState::load(deps.storage)?;
                    ob_state.liquidity_percent = liquidity_percent;
                    ob_state.min_base_order_size = min_base_order_size;
                    ob_state.min_quote_order_size = min_quote_order_size;
                    ob_state.orders_number = orders_number;
                    ob_state.save(deps.storage)?;
                }
                _ => {
                    return Err(StdError::generic_err(format!(
                        "Can't migrate from {} {}",
                        contract_info.contract, contract_info.version
                    )));
                }
            }
        }
        MigrateMsg::Migrate {} => {
            let contract_info = cw2::get_contract_version(deps.storage)?;
            match contract_info.contract.as_str() {
                CONTRACT_NAME => match contract_info.version.as_str() {
                    "2.0.3" | "2.0.4" => {}
                    _ => {
                        return Err(StdError::generic_err(format!(
                            "Can't migrate from {} {}",
                            contract_info.contract, contract_info.version
                        )));
                    }
                },
                _ => {
                    return Err(StdError::generic_err(format!(
                        "Can't migrate from {} {}",
                        contract_info.contract, contract_info.version
                    )));
                }
            }
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    attrs.extend([
        attr("previous_contract_name", contract_info.contract),
        attr("previous_contract_version", contract_info.version),
        attr("new_contract_name", CONTRACT_NAME),
        attr("new_contract_version", CONTRACT_VERSION),
    ]);
    Ok(Response::default().add_attributes(attrs))
}
