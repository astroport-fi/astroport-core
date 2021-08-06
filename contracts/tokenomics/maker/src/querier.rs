use cosmwasm_std::{DepsMut, Deps, Addr, StdResult, QueryRequest, WasmQuery, to_binary, QuerierWrapper};
use std::convert::TryFrom;
use crate::msg::QueryMsg;
use terraswap::asset::{AssetInfo, PairInfo};

pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}


