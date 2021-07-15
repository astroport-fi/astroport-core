use cosmwasm_std::{to_binary, Deps, QueryRequest, StdResult, WasmQuery};
use terraswap::asset::PairInfo;
use terraswap::pair::QueryMsg;

pub fn query_liquidity_token(deps: Deps, contract_addr: &String) -> StdResult<String> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.clone(),
        msg: to_binary(&QueryMsg::Pair {})?,
    }))?;

    Ok(res.liquidity_token.to_string())
}
