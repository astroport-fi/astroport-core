use astroport::asset::{Asset, AssetInfo};
use astroport::pair::{CumulativePricesResponse, QueryMsg as PairQueryMsg, SimulationResponse};
use cosmwasm_std::{QuerierWrapper, StdResult};

/// Returns information about a pair's asset cumulative prices using a [`CumulativePricesResponse`] object.
///
/// * **pair_contract** address of the pair for which we return data.
pub fn query_cumulative_prices(
    querier: QuerierWrapper,
    pair_contract: impl Into<String>,
) -> StdResult<CumulativePricesResponse> {
    querier.query_wasm_smart(pair_contract, &PairQueryMsg::CumulativePrices {})
}

/// Returns information about an asset's price from a specific pair.
///
/// * **pair_contract** pair that holds the target asset.
///
/// * **asset** asset for which we return the simulated price.
pub fn query_prices(
    querier: QuerierWrapper,
    pair_contract: impl Into<String>,
    offer_asset: Asset,
    ask_asset_info: Option<AssetInfo>,
) -> StdResult<SimulationResponse> {
    querier.query_wasm_smart(
        pair_contract,
        &PairQueryMsg::Simulation {
            offer_asset,
            ask_asset_info,
        },
    )
}
