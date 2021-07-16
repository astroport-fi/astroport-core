use crate::asset::{Asset, AssetInfo, PairInfo};
use crate::factory::QueryMsg as FactoryQueryMsg;
use crate::pair::{QueryMsg as PairQueryMsg, ReverseSimulationResponse, SimulationResponse};

use cosmwasm_std::{
    to_binary, Addr, AllBalanceResponse, BalanceResponse, BankQuery, Coin, QuerierWrapper,
    QueryRequest, StdResult, Uint128, WasmQuery,
};

use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg, TokenInfoResponse};

pub fn query_balance(
    querier: &QuerierWrapper,
    account_addr: Addr,
    denom: String,
) -> StdResult<Uint128> {
    let balance: BalanceResponse = querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: String::from(account_addr),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

pub fn query_all_balances(querier: &QuerierWrapper, account_addr: Addr) -> StdResult<Vec<Coin>> {
    let all_balances: AllBalanceResponse =
        querier.query(&QueryRequest::Bank(BankQuery::AllBalances {
            address: String::from(account_addr),
        }))?;
    Ok(all_balances.amount)
}

pub fn query_token_balance(
    querier: &QuerierWrapper,
    contract_addr: Addr,
    account_addr: Addr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    let res: Cw20BalanceResponse = querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&Cw20QueryMsg::Balance {
                address: String::from(account_addr),
            })?,
        }))
        .unwrap_or_else(|_| Cw20BalanceResponse {
            balance: Uint128::zero(),
        });

    Ok(res.balance)
}

pub fn query_supply(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<Uint128> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.total_supply)
}

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

pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
        })?,
    }))
}

pub fn reverse_simulate(
    querier: &QuerierWrapper,
    pair_contract: &Addr,
    ask_asset: &Asset,
) -> StdResult<ReverseSimulationResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::ReverseSimulation {
            ask_asset: ask_asset.clone(),
        })?,
    }))
}
