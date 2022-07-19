use crate::asset::{Asset, AssetInfo, PairInfo};
use crate::factory::{
    ConfigResponse as FactoryConfigResponse, FeeInfoResponse, PairType, PairsResponse,
    QueryMsg as FactoryQueryMsg,
};
use crate::pair::{QueryMsg as PairQueryMsg, ReverseSimulationResponse, SimulationResponse};

use cosmwasm_std::{
    to_binary, Addr, AllBalanceResponse, BalanceResponse, BankQuery, Coin, Decimal, QuerierWrapper,
    QueryRequest, StdResult, Uint128, WasmQuery,
};

use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg, TokenInfoResponse};

// It's defined at https://github.com/terra-money/core/blob/d8e277626e74f9d6417dcd598574686882f0274c/types/assets/assets.go#L15
const NATIVE_TOKEN_PRECISION: u8 = 6;

/// Returns a native token's balance for a specific account.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **account_addr** is an object of type [`Addr`].
///
/// * **denom** is an object of type [`String`] used to specify the denomination used to return the balance (e.g uluna).
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

/// Returns the total balances for all coins at a specified account address.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **account_addr** is an object of type [`Addr`] which is the address for which we query balances.
pub fn query_all_balances(querier: &QuerierWrapper, account_addr: Addr) -> StdResult<Vec<Coin>> {
    let all_balances: AllBalanceResponse =
        querier.query(&QueryRequest::Bank(BankQuery::AllBalances {
            address: String::from(account_addr),
        }))?;
    Ok(all_balances.amount)
}

/// Returns a token balance for an account.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract_addr** is an object of type [`Addr`]. This is the token contract for which we return a balance.
///
/// * **account_addr** is an object of type [`Addr`] for which we query the token balance for.
pub fn query_token_balance(
    querier: &QuerierWrapper,
    contract_addr: Addr,
    account_addr: Addr,
) -> StdResult<Uint128> {
    // load balance from the token contract
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

/// Returns a token's symbol.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract_addr** is an object of type [`Addr`] which is the token contract address.
pub fn query_token_symbol(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<String> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.symbol)
}

/// Returns the total supply of a specific token.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract_addr** is an object of type [`Addr`] which is the token contract address.
pub fn query_supply(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<Uint128> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.total_supply)
}

/// Returns the number of decimals that a token has.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **asset_info** is an object of type [`AssetInfo`] and contains the asset details for a specific token.
pub fn query_token_precision(querier: &QuerierWrapper, asset_info: AssetInfo) -> StdResult<u8> {
    Ok(match asset_info {
        AssetInfo::NativeToken { denom: _ } => NATIVE_TOKEN_PRECISION,
        AssetInfo::Token { contract_addr } => {
            let res: TokenInfoResponse =
                querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

            res.decimals
        }
    })
}

/// Returns the configuration for the factory contract.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`] which is the Astroport factory contract address.
pub fn query_factory_config(
    querier: &QuerierWrapper,
    factory_contract: Addr,
) -> StdResult<FactoryConfigResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Config {})?,
    }))
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
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`].
///
/// * **pair_type** is an object of type [`PairType`]. This is the pair type we return information for.
pub fn query_fee_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    pair_type: PairType,
) -> StdResult<FeeInfo> {
    let res: FeeInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::FeeInfo { pair_type })?,
    }))?;

    Ok(FeeInfo {
        fee_address: res.fee_address,
        total_fee_rate: Decimal::from_ratio(Uint128::from(res.total_fee_bps), Uint128::new(10000)),
        maker_fee_rate: Decimal::from_ratio(Uint128::from(res.maker_fee_bps), Uint128::new(10000)),
    })
}

/// Accepts two tokens as input and returns a pair's information.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`] and it's the Astroport factory contract address
///
/// * **asset_infos** is an array that contains two items of type [`AssetInfo`].
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

/// Returns a vector that contains items of type [`PairInfo`] which symbolize pairs instantiated in the Astroport factory
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`] and represents the Astroport factory contract address.
///
/// * **start_after** is an [`Option`] field that contains an array with two items of type [`AssetInfo`].
///
/// * **limit** is an [`Option`] field of type [`u32`] which is the maximum amount of pairs for which to return information.
pub fn query_pairs_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> StdResult<PairsResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Pairs { start_after, limit })?,
    }))
}

/// Returns information about a swap simulation using a [`SimulationResponse`] object.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **pair_contract** is an object of type [`Addr`] and represents the address of the pair for which we return swap simulation info.
///
/// * **offer_asset** is an object of type [`Asset`] and represents the asset that is being swapped.
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

/// Returns information about a reverse swap simulation using a [`ReverseSimulationResponse`] object.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **pair_contract** is an object of type [`Addr`] and represents the address of the pair for which we return swap simulation info.
///
/// * **ask_asset** is an object of type [`Asset`] and represents the asset that we swap to.
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
