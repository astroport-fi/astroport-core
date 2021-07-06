use crate::asset::{Asset, AssetInfo, PairInfo};
use crate::factory::QueryMsg as FactoryQueryMsg;
use crate::pair::{QueryMsg as PairQueryMsg, ReverseSimulationResponse, SimulationResponse};

use cosmwasm_std::{
    to_binary, AllBalanceResponse, Api, BalanceResponse, BankQuery, Coin,
    Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage, Uint128, WasmQuery,
};
use cw20::{TokenInfoResponse, Cw20QueryMsg, BalanceResponse as Cw20BalanceResponse};

pub fn query_balance<S: Storage, A: Api, Q: Querier>(
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

pub fn query_all_balances<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_addr: &HumanAddr,
) -> StdResult<Vec<Coin>> {
    // load price form the oracle
    let all_balances: AllBalanceResponse =
        deps.querier
            .query(&QueryRequest::Bank(BankQuery::AllBalances {
                address: HumanAddr::from(account_addr),
            }))?;
    Ok(all_balances.amount)
}

pub fn query_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    account_addr: &HumanAddr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    // let res: Binary = deps
    //     .querier
    //     .query(&QueryRequest::Wasm(WasmQuery::Raw {
    //         contract_addr: HumanAddr::from(contract_addr),
    //         key: Binary::from(concat(
    //             &to_length_prefixed(b"balance").to_vec(),
    //             (deps.api.canonical_address(&account_addr)?).as_slice(),
    //         )),
    //     }))
    //     .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());
    //
    // from_binary(&res)

    let res: Cw20BalanceResponse = deps
        .querier
        .query(&QueryRequest::Wasm(
            WasmQuery::Smart {
            contract_addr: HumanAddr::from(contract_addr),
                msg: to_binary(&Cw20QueryMsg::Balance {
                    address:  HumanAddr::from(account_addr),
                })?,
        }))
        .unwrap_or_else(|_| Cw20BalanceResponse{ balance: Uint128::zero()});

    Ok(res.balance)
}

pub fn query_supply<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<Uint128> {
    // load price form the oracle
    // let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
    //     contract_addr: HumanAddr::from(contract_addr),
    //     key: Binary::from(to_length_prefixed(b"token_info")),
    // }))?;
    // let token_info: TokenInfoResponse = from_binary(&res)?;
    // Ok(token_info.total_supply)
    let res: TokenInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: HumanAddr::from(contract_addr),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;
    Ok(res.total_supply)
}

pub fn query_pair_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    factory_contract: &HumanAddr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.clone(),
        msg: to_binary(&FactoryQueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}

pub fn simulate<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pair_contract: &HumanAddr,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.clone(),
        msg: to_binary(&PairQueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
        })?,
    }))
}

pub fn reverse_simulate<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pair_contract: &HumanAddr,
    ask_asset: &Asset,
) -> StdResult<ReverseSimulationResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.clone(),
        msg: to_binary(&PairQueryMsg::ReverseSimulation {
            ask_asset: ask_asset.clone(),
        })?,
    }))
}
