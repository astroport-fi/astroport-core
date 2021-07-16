use cosmwasm_std::{to_binary, Addr, Deps, QueryRequest, StdResult, WasmQuery};
use terraswap::asset::PairInfo;
use terraswap::pair::QueryMsg;

pub fn query_liquidity_token(deps: Deps, contract_addr: Addr) -> StdResult<Addr> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(&QueryMsg::Pair {})?,
    }))?;

    Ok(res.liquidity_token)
}
