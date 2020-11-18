use cosmwasm_std::{
    from_binary, Api, Binary, Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage,
    WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use terraswap::PairInfoRaw;

pub fn query_liquidity_token<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<HumanAddr> {
    // load price form the oracle
    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.clone(),
        key: Binary::from(to_length_prefixed(b"pair_info")),
    }))?;

    let pair_info: PairInfoRaw = from_binary(&res)?;
    deps.api.human_address(&pair_info.liquidity_token)
}
