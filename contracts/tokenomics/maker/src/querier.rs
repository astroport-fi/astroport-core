use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::SimulationResponse;
use cosmwasm_std::{to_binary, Addr, QuerierWrapper, QueryRequest, StdResult, Uint128, WasmQuery};

pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&astroport::factory::QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}

pub fn query_pair_share(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    share: Uint128,
) -> StdResult<Vec<Asset>> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&astroport::pair::QueryMsg::Share { amount: share })?,
    }))
}

pub fn query_swap_amount(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    asset_info: AssetInfo,
    amount: Uint128,
) -> StdResult<Uint128> {
    let asset = Asset {
        info: asset_info,
        amount,
    };

    let response: SimulationResponse = querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&astroport::pair::QueryMsg::Simulation { offer_asset: asset })?,
        }))
        .unwrap();

    Ok(response.return_amount)
}

// pub fn query_swap_reverse_amount(
//     querier: &QuerierWrapper,
//     pair_contract: Addr,
//     asset_info: AssetInfo,
//     amount: Uint128
// )-> StdResult<Uint128> {
//
//     let asset = Asset{ info: asset_info, amount };
//
//     let response:ReverseSimulationResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//         contract_addr: pair_contract.to_string(),
//         msg: to_binary(&terraswap::pair::QueryMsg::ReverseSimulation {
//             ask_asset: asset,
//         })?,
//     })).unwrap();
//
//     Ok(response.offer_amount)
// }
