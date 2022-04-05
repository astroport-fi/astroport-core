use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::QueryMsg as FactoryQueryMsg;
use astroport::pair::{CumulativePricesResponse, QueryMsg as PairQueryMsg, SimulationResponse};
use cosmwasm_std::{to_binary, Addr, QuerierWrapper, QueryRequest, StdResult, WasmQuery};

/// ## Description
/// Returns information about the target pair using a [`PairInfo`] object.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`]. This is the Astroport factory contract address.
///
/// * **asset_infos** is an array with two items of type [`AssetInfo`]. These objects holds information about two assets in an Astroport pool.
pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    asset_infos: [AssetInfo; 2],
) -> StdResult<PairInfo> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Pair { asset_infos })?,
    }))
}

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
