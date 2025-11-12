#![cfg(not(tarpaulin_include))]

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use crate::state::PAIRS;
use astroport::pair;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, DepsMut, Empty, Env, Order, Response, StdResult};
use cw_storage_plus::Map;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = cw2::get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "1.10.0" => {
                deps.storage.remove(b"tmp_pair_info");
                deps.storage.remove(b"tracker_config");

                // Migration is very gas-intensive.
                // However, it fits in current gas per block limits
                // on Neutron (330 M) and Terra (100 M).
                let old_pairs_iface = Map::<&[u8], Addr>::new("pair_info");
                let pools = old_pairs_iface
                    .range_raw(deps.storage, None, None, Order::Ascending)
                    .map(|item| Ok(item?.1))
                    .collect::<StdResult<Vec<_>>>()?;

                old_pairs_iface.clear(deps.storage);

                for pool in pools {
                    let pair_info = deps
                        .querier
                        .query_wasm_smart(&pool, &pair::QueryMsg::Pair {})?;

                    PAIRS.save(deps.storage, &pool, &pair_info)?;
                }
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
