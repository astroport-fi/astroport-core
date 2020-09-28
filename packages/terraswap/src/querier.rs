use cosmwasm_std::{
    from_binary, to_binary, Api, BalanceResponse, BankQuery, Binary, Extern, HumanAddr, Querier,
    QueryRequest, StdResult, Storage, Uint128, WasmQuery,
};

use crate::asset::{AssetInfo, PairInfoRaw};
use crate::init::PairConfigRaw;
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

pub fn load_pair_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<HumanAddr> {
    let mut asset_infos = [asset_infos[0].to_raw(&deps)?, asset_infos[1].to_raw(&deps)?];

    asset_infos.sort_by(|a, b| a.as_bytes().cmp(&b.as_bytes()));

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: HumanAddr::from(contract_addr),
        key: Binary::from(concat(
            &to_length_prefixed(b"pair").to_vec(),
            &[asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat(),
        )),
    }))?;

    let pair_info: PairInfoRaw = from_binary(&res)?;
    deps.api.human_address(&pair_info.contract_addr)
}

pub fn load_liquidity_token<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<HumanAddr> {
    // load price form the oracle
    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.clone(),
        key: Binary::from(concat(&to_length_prefixed(b"config"), b"general")),
    }))?;

    let config: PairConfigRaw = from_binary(&res)?;
    deps.api.human_address(&config.liquidity_token)
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
