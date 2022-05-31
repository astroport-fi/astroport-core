use astroport::asset::Asset;
use astroport::pair::{CumulativePricesResponse, QueryMsg as PairQueryMsg, SimulationResponse};
use cosmwasm_std::{to_binary, Addr, QuerierWrapper, QueryRequest, StdResult, WasmQuery};

/// ## Description
/// Returns information about a pair's asset cumulative prices using a [`CumulativePricesResponse`] object.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **pair_contract** is an object of type [`Addr`]. This is the address of the pair for which we return data.
pub fn query_cumulative_prices(
    querier: &QuerierWrapper,
    pair_contract: Addr,
) -> StdResult<CumulativePricesResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::CumulativePrices {})?,
    }))
}

/// ## Description
/// Returns information about an asset's price from a specific pair using a [`SimulationResponse`] object.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **pair_contract** is an object of type [`Addr`]. This is the pair that holds the target asset.
///
/// * **asset** is an object of type [`Asset`]. This is the asset for which we return the simulated price.
pub fn query_prices(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    asset: Asset,
) -> StdResult<SimulationResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::Simulation { offer_asset: asset })?,
    }))
}
