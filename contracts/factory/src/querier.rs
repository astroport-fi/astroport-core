use astroport::asset::PairInfo;
use astroport::pair::QueryMsg;
use cosmwasm_std::{QuerierWrapper, StdResult};

/// Returns information about a pair (using the [`PairInfo`] struct).
///
/// `pair_contract` is the pair for which to retrieve information.
pub fn query_pair_info(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
) -> StdResult<PairInfo> {
    querier.query_wasm_smart(pair_contract, &QueryMsg::Pair {})
}
