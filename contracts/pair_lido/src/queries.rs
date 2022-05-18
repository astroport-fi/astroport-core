// SPDX-License-Identifier: GPL-3.0-only
// Copyright Lido

use cosmwasm_std::{to_binary, Addr, Deps, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{Cw20QueryMsg, TokenInfoResponse};

/// ## Description
/// Returns current parameters of the Lido Hub
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **hub_address** is the object of type [`Addr`].
pub fn query_hub_params(deps: Deps, hub_address: Addr) -> StdResult<basset::hub::Parameters> {
    let params: basset::hub::Parameters =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: hub_address.to_string(),
            msg: to_binary(&basset::hub::QueryMsg::Parameters {})?,
        }))?;
    Ok(params)
}

/// ## Description
/// Returns current state of the Lido Hub
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **hub_address** is the object of type [`Addr`].
pub fn query_hub_state(deps: Deps, hub_address: Addr) -> StdResult<basset::hub::StateResponse> {
    let state: basset::hub::StateResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: hub_address.to_string(),
            msg: to_binary(&basset::hub::QueryMsg::State {})?,
        }))?;
    Ok(state)
}

/// ## Description
/// Returns current batch of the Lido Hub
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **hub_address** is the object of type [`Addr`].
pub fn query_current_batch(
    deps: Deps,
    hub_address: Addr,
) -> StdResult<basset::hub::CurrentBatchResponse> {
    let batch: basset::hub::CurrentBatchResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: hub_address.to_string(),
            msg: to_binary(&basset::hub::QueryMsg::CurrentBatch {})?,
        }))?;
    Ok(batch)
}

/// ## Description
/// Returns total issued CW20 tokens amount
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **token_address** is the object of type [`Addr`].
pub fn query_total_tokens_issued(deps: Deps, token_address: Addr) -> StdResult<Uint128> {
    let token_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token_address.to_string(),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;
    Ok(token_info.total_supply)
}
