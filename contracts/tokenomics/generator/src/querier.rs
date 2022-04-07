use crate::error::ContractError;
use crate::state::Config;
use astroport_governance::generator_controller::{ConfigResponse, QueryMsg};
use cosmwasm_std::DepsMut;

/// Returns information about the generator controller
pub(crate) fn query_generator_controller_info(
    deps: DepsMut,
    cfg: &Config,
) -> Result<ConfigResponse, ContractError> {
    if let Some(generator_controller) = &cfg.generator_controller {
        let resp: ConfigResponse = deps
            .querier
            .query_wasm_smart(generator_controller, &QueryMsg::Config {})?;
        Ok(resp)
    } else {
        Err(ContractError::GeneratorControllerNotFound {})
    }
}
