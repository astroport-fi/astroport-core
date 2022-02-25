use astroport::asset::PairInfo;
use astroport::pair::QueryMsg;
use cosmwasm_std::{to_binary, Addr, Deps, QueryRequest, StdResult, WasmQuery};

/// ## Description
/// Returns information about a pair (using the [`PairInfo`] struct).
/// ## Params
/// `pair_contract` is a param of type [`Addr`]. This is the pair for which to retrieve information.
pub fn query_pair_info(deps: Deps, pair_contract: &Addr) -> StdResult<PairInfo> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&QueryMsg::Pair {})?,
    }))
}
