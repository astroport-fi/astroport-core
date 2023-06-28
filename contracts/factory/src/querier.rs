use astroport::asset::PairInfo;
use astroport::pair::QueryMsg;
use classic_bindings::TerraQuery;
use cosmwasm_std::{QuerierWrapper, StdResult};

/// ## Description
/// Returns information about the pair described in the structure [`PairInfo`] according to the specified parameters in the `pair_contract` variable.
/// ## Params
/// `pair_contract` it is the type of [`Addr`].
pub fn query_pair_info(
    querier: &QuerierWrapper<TerraQuery>,
    pair_contract: impl Into<String>,
) -> StdResult<PairInfo> {
    querier.query_wasm_smart(pair_contract, &QueryMsg::Pair {})
}
