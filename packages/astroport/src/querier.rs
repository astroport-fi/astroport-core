use crate::asset::{Asset, AssetInfo, PairInfo};
use crate::factory::{
    Config as FactoryConfig, FeeInfoResponse, PairType, PairsResponse, QueryMsg as FactoryQueryMsg,
    TrackerConfig,
};
use crate::pair::{QueryMsg as PairQueryMsg, ReverseSimulationResponse, SimulationResponse};

use cosmwasm_std::{
    from_json, Addr, AllBalanceResponse, BankQuery, Coin, CustomQuery, Decimal, QuerierWrapper,
    QueryRequest, StdError, StdResult, Uint128,
};

use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg, TokenInfoResponse};

/// Returns a native token's balance for a specific account.
///
/// * **denom** specifies the denomination used to return the balance (e.g uluna).
pub fn query_balance<C>(
    querier: &QuerierWrapper<C>,
    account_addr: impl Into<String>,
    denom: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    querier
        .query_balance(account_addr, denom)
        .map(|coin| coin.amount)
}

/// Returns the total balances for all coins at a specified account address.
///
/// * **account_addr** address for which we query balances.
pub fn query_all_balances(querier: &QuerierWrapper, account_addr: Addr) -> StdResult<Vec<Coin>> {
    let all_balances: AllBalanceResponse =
        querier.query(&QueryRequest::Bank(BankQuery::AllBalances {
            address: String::from(account_addr),
        }))?;
    Ok(all_balances.amount)
}

/// Returns a token balance for an account.
///
/// * **contract_addr** token contract for which we return a balance.
///
/// * **account_addr** account address for which we return a balance.
pub fn query_token_balance<C>(
    querier: &QuerierWrapper<C>,
    contract_addr: impl Into<String>,
    account_addr: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    // load balance from the token contract
    let resp: Cw20BalanceResponse = querier
        .query_wasm_smart(
            contract_addr,
            &Cw20QueryMsg::Balance {
                address: account_addr.into(),
            },
        )
        .unwrap_or_else(|_| Cw20BalanceResponse {
            balance: Uint128::zero(),
        });

    Ok(resp.balance)
}

/// Returns a token's symbol.
///
/// * **contract_addr** token contract address.
pub fn query_token_symbol<C>(
    querier: &QuerierWrapper<C>,
    contract_addr: impl Into<String>,
) -> StdResult<String>
where
    C: CustomQuery,
{
    let res: TokenInfoResponse =
        querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

    Ok(res.symbol)
}

/// Returns the total supply of a specific token.
///
/// * **contract_addr** token contract address.
pub fn query_supply<C>(
    querier: &QuerierWrapper<C>,
    contract_addr: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    let res: TokenInfoResponse =
        querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

    Ok(res.total_supply)
}

/// Returns the total supply of a native token.
///
/// * **denom** specifies the denomination used to return the supply (e.g uatom).
pub fn query_native_supply<C>(
    querier: &QuerierWrapper<C>,
    denom: impl Into<String>,
) -> StdResult<Uint128>
where
    C: CustomQuery,
{
    querier.query_supply(denom).map(|res| res.amount)
}

/// Returns the number of decimals that a token has.
///
/// * **asset_info** is an object of type [`AssetInfo`] and contains the asset details for a specific token.
pub fn query_token_precision<C>(
    querier: &QuerierWrapper<C>,
    asset_info: &AssetInfo,
    factory_addr: &Addr,
) -> StdResult<u8>
where
    C: CustomQuery,
{
    Ok(match asset_info {
        AssetInfo::NativeToken { denom } => {
            let res = query_factory_config(querier, factory_addr)?;
            let result = crate::native_coin_registry::COINS_INFO.query(
                querier,
                res.coin_registry_address,
                denom.to_string(),
            )?;

            if let Some(decimals) = result {
                decimals
            } else {
                return Err(StdError::generic_err(format!(
                    "The {denom} precision was not found"
                )));
            }
        }
        AssetInfo::Token { contract_addr } => {
            let res: TokenInfoResponse =
                querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

            res.decimals
        }
    })
}

/// Returns the configuration for the factory contract.
pub fn query_factory_config<C>(
    querier: &QuerierWrapper<C>,
    factory_contract: impl Into<String>,
) -> StdResult<FactoryConfig>
where
    C: CustomQuery,
{
    if let Some(res) = querier.query_wasm_raw(factory_contract, b"config".as_slice())? {
        let res = from_json(res)?;
        Ok(res)
    } else {
        Err(StdError::generic_err("The factory config not found!"))
    }
}

/// Returns the tracker configuration from the factory contract.
pub fn query_tracker_config<C>(
    querier: &QuerierWrapper<C>,
    factory_contract: impl Into<String>,
) -> StdResult<TrackerConfig>
where
    C: CustomQuery,
{
    if let Some(res) = querier.query_wasm_raw(factory_contract, b"tracker_config".as_slice())? {
        Ok(from_json(res)?)
    } else {
        Err(StdError::generic_err("The tracker config not found!"))
    }
}

/// This structure holds parameters that describe the fee structure for a pool.
pub struct FeeInfo {
    /// The fee address
    pub fee_address: Option<Addr>,
    /// The total amount of fees charged per swap
    pub total_fee_rate: Decimal,
    /// The amount of fees sent to the Maker contract
    pub maker_fee_rate: Decimal,
}

/// Returns the fee information for a specific pair type.
///
/// * **pair_type** pair type we query information for.
pub fn query_fee_info<C>(
    querier: &QuerierWrapper<C>,
    factory_contract: impl Into<String>,
    pair_type: PairType,
) -> StdResult<FeeInfo>
where
    C: CustomQuery,
{
    let res: FeeInfoResponse =
        querier.query_wasm_smart(factory_contract, &FactoryQueryMsg::FeeInfo { pair_type })?;

    Ok(FeeInfo {
        fee_address: res.fee_address,
        total_fee_rate: Decimal::from_ratio(res.total_fee_bps, 10000u16),
        maker_fee_rate: Decimal::from_ratio(res.maker_fee_bps, 10000u16),
    })
}

/// Accepts two tokens as input and returns a pair's information.
pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: impl Into<String>,
    asset_infos: &[AssetInfo],
) -> StdResult<PairInfo> {
    querier.query_wasm_smart(
        factory_contract,
        &FactoryQueryMsg::Pair {
            asset_infos: asset_infos.to_vec(),
        },
    )
}

/// Returns a vector that contains items of type [`PairInfo`] which
/// symbolize pairs instantiated in the Astroport factory
pub fn query_pairs_info(
    querier: &QuerierWrapper,
    factory_contract: impl Into<String>,
    start_after: Option<Vec<AssetInfo>>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    querier.query_wasm_smart(
        factory_contract,
        &FactoryQueryMsg::Pairs { start_after, limit },
    )
}

/// Returns information about a swap simulation using a [`SimulationResponse`] object.
///
/// * **pair_contract** address of the pair for which we return swap simulation info.
///
/// * **offer_asset** asset that is being swapped.
pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse> {
    querier.query_wasm_smart(
        pair_contract,
        &PairQueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
            ask_asset_info: None,
        },
    )
}

/// Returns information about a reverse swap simulation using a [`ReverseSimulationResponse`] object.
///
/// * **pair_contract**  address of the pair for which we return swap simulation info.
///
/// * **ask_asset** represents the asset that we swap to.
pub fn reverse_simulate(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
    ask_asset: &Asset,
) -> StdResult<ReverseSimulationResponse> {
    querier.query_wasm_smart(
        pair_contract,
        &PairQueryMsg::ReverseSimulation {
            offer_asset_info: None,
            ask_asset: ask_asset.clone(),
        },
    )
}
