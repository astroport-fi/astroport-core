use cosmwasm_std::{Addr, StdResult, QueryRequest, WasmQuery, to_binary, QuerierWrapper};
use terraswap::asset::{AssetInfo, PairInfo};

pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&terraswap::factory::QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}


