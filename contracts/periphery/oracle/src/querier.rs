use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::QueryMsg as FactoryQueryMsg;
use astroport::pair::{CumulativePricesResponse, QueryMsg as PairQueryMsg, SimulationResponse};
use cosmwasm_std::{to_binary, Addr, QuerierWrapper, QueryRequest, StdResult, WasmQuery};

/// ## Description
/// Returns information about the pair in a [`PairInfo`] object.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **factory_contract** is the object of type [`Addr`].
///
/// * **asset_infos** is array with two items the type of [`AssetInfo`].
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
/// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **pair_contract** is the object of type [`Addr`].
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
/// Returns information about the prices in a [`SimulationResponse`] object.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **pair_contract** is the object of type [`Addr`].
///
/// * **asset** is the object of type [`Asset`].
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
