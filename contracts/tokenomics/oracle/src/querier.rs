use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::QueryMsg as FactoryQueryMsg;
use astroport::pair::{CumulativePricesResponse, QueryMsg as PairQueryMsg};
use cosmwasm_std::{to_binary, Addr, QuerierWrapper, QueryRequest, StdResult, WasmQuery};

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

pub fn query_cumulative_prices(
    querier: &QuerierWrapper,
    pair_contract: Addr,
) -> StdResult<CumulativePricesResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::CumulativePrices {})?,
    }))
}
