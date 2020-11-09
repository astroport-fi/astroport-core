use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{AssetInfo, PairInfo};
use cosmwasm_std::{
    from_binary, to_binary, Api, BalanceResponse, BankQuery, Binary, Extern, HumanAddr, Querier,
    QueryRequest, StdResult, Storage, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20::TokenInfoResponse;

pub fn load_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_addr: &HumanAddr,
    denom: String,
) -> StdResult<Uint128> {
    // load price form the oracle
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: HumanAddr::from(account_addr),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

pub fn load_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    account_addr: &HumanAddr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    let res: Binary = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: HumanAddr::from(contract_addr),
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                (deps.api.canonical_address(&account_addr)?).as_slice(),
            )),
        }))
        .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());

    from_binary(&res)
}

pub fn load_supply<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<Uint128> {
    // load price form the oracle
    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: HumanAddr::from(contract_addr),
        key: Binary::from(to_length_prefixed(b"token_info")),
    }))?;

    let token_info: TokenInfoResponse = from_binary(&res)?;
    Ok(token_info.total_supply)
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Pair { asset_infos: [AssetInfo; 2] },
}

pub fn load_pair_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    factory_contract: &HumanAddr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.clone(),
        msg: to_binary(&QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}
