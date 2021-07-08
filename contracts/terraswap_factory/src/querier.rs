use cosmwasm_std::{
    to_binary, Api, Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage, WasmQuery,
};
use terraswap::asset::PairInfo;
use terraswap::pair::QueryMsg;

pub fn query_liquidity_token<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<HumanAddr> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.clone(),
        msg: to_binary(&QueryMsg::Pair {})?,
    }))?;

    Ok(res.liquidity_token)
}
