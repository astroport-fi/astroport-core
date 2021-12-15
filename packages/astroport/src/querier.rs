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

/// ## Description
/// Returns the balance of the denom at the specified account address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **account_addr** is the object of type [`Addr`].
///
/// * **denom** is the object of type [`String`].
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

/// ## Description
/// Returns the total balance for all coins at the specified account address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **account_addr** is the object of type [`Addr`].
pub fn query_all_balances(querier: &QuerierWrapper, account_addr: Addr) -> StdResult<Vec<Coin>> {
    let all_balances: AllBalanceResponse =
        querier.query(&QueryRequest::Bank(BankQuery::AllBalances {
            address: String::from(account_addr),
        }))?;
    Ok(all_balances.amount)
}

/// ## Description
/// Returns the token balance at the specified contract address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **contract_addr** is the object of type [`Addr`]. Sets the address of the contract for which
/// the balance will be requested
///
/// * **account_addr** is the object of type [`Addr`].
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

/// ## Description
/// Returns the token symbol at the specified contract address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **contract_addr** is the object of type [`Addr`].
pub fn query_token_symbol(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<String> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.symbol)
}

/// ## Description
/// Returns the total supply at the specified contract address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **contract_addr** is the object of type [`Addr`].
pub fn query_supply(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<Uint128> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.total_supply)
}

/// ## Description
/// Returns the token precision at the specified asset of type [`AssetInfo`].
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **asset_info** is the object of type [`AssetInfo`].
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

/// ## Description
/// Returns the config of factory contract address.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **factory_contract** is the object of type [`Addr`].
pub fn query_factory_config(
    querier: &QuerierWrapper,
    factory_contract: Addr,
) -> StdResult<FactoryConfigResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Config {})?,
    }))
}

/// ## Description
/// This structure describes the basic fee information.
pub struct FeeInfo {
    /// the fee address
    pub fee_address: Option<Addr>,
    /// the total fee rate
    pub total_fee_rate: Decimal,
    /// the maker fee rate
    pub maker_fee_rate: Decimal,
}

/// ## Description
/// Returns the fee information at the specified pair type.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **factory_contract** is the object of type [`Addr`].
///
/// * **pair_type** is the object of type [`PairType`].
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

/// ## Description
/// Returns the pair information at the specified assets of type [`AssetInfo`].
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **factory_contract** is the object of type [`Addr`].
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

/// ## Description
/// Returns the vector that contains items of type [`PairInfo`]
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **factory_contract** is the object of type [`Addr`].
///
/// * **start_after** is an [`Option`] field that contains array with two items of type [`AssetInfo`].
///
/// * **limit** is an [`Option`] field of type [`u32`].
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

/// ## Description
/// Returns information about the simulation of the swap in a [`SimulationResponse`] object.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **pair_contract** is the object of type [`Addr`].
///
/// * **offer_asset** is the object of type [`Asset`].
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

/// ## Description
/// Returns information about the reverse simulation in a [`ReverseSimulationResponse`] object.
/// ## Params
/// * **querier** is the object of type [`QuerierWrapper`].
///
/// * **pair_contract** is the object of type [`Addr`].
///
/// * **ask_asset** is the object of type [`Asset`].
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
