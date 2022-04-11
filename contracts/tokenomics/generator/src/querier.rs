use astroport_governance::generator_controller::{ConfigResponse, QueryMsg};
use cosmwasm_std::{Addr, DepsMut, StdResult};

/// Returns information about the generator controller
pub(crate) fn query_generator_controller_info(
    deps: DepsMut,
    generator_controller: &Addr,
) -> StdResult<ConfigResponse> {
    deps.querier
        .query_wasm_smart(generator_controller.to_string(), &QueryMsg::Config {})
}
