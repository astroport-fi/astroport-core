use astroport::asset::{Asset, AssetInfo};
use astroport::pair::{ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg};
use astroport::querier::{query_balance, query_pair_info, query_token_balance};
use astroport::router::SwapOperation;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::state::CONFIG;

/// Execute a swap operation.
///
/// * **operation** to perform (factory or direct pool swap with offer and ask asset information).
///
/// * **to** address that receives the ask assets.
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<String>,
) -> Result<Response, ContractError> {
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let (pool_addr, offer_asset_info, ask_asset_info) = resolve_pool(deps.as_ref(), &operation)?;

    let amount = match &offer_asset_info {
        AssetInfo::NativeToken { denom } => {
            query_balance(&deps.querier, env.contract.address, denom)?
        }
        AssetInfo::Token { contract_addr } => {
            query_token_balance(&deps.querier, contract_addr, env.contract.address)?
        }
    };
    let offer_asset = Asset {
        info: offer_asset_info,
        amount,
    };

    let message = asset_into_swap_msg(pool_addr.to_string(), offer_asset, ask_asset_info, to)?;

    Ok(Response::new().add_message(message))
}

/// Returns the pool to swap in and the operation's offer and ask assets.
///
/// An [`SwapOperation::AstroSwap`] pool is looked up in the factory by its asset pair. A
/// [`SwapOperation::PoolSwap`] pool is used as given, after checking that it holds both assets.
pub fn resolve_pool(
    deps: Deps,
    operation: &SwapOperation,
) -> Result<(Addr, AssetInfo, AssetInfo), ContractError> {
    match operation {
        SwapOperation::AstroSwap {
            offer_asset_info,
            ask_asset_info,
        } => {
            let config = CONFIG.load(deps.storage)?;
            let pair_info = query_pair_info(
                &deps.querier,
                config.astroport_factory,
                &[offer_asset_info.clone(), ask_asset_info.clone()],
            )?;

            Ok((
                pair_info.contract_addr,
                offer_asset_info.clone(),
                ask_asset_info.clone(),
            ))
        }
        SwapOperation::PoolSwap {
            pool_addr,
            offer_asset_info,
            ask_asset_info,
        } => {
            let pool_addr = deps.api.addr_validate(pool_addr)?;
            let pair_info: astroport::asset::PairInfo = deps
                .querier
                .query_wasm_smart(&pool_addr, &PairQueryMsg::Pair {})?;

            // catches proxies and pasted addresses that report another pool's info
            ensure!(
                pair_info.contract_addr == pool_addr,
                ContractError::PoolAddressMismatch {
                    pool: pool_addr.to_string(),
                    reported: pair_info.contract_addr.to_string(),
                }
            );

            ensure!(
                pair_info.asset_infos.contains(offer_asset_info)
                    && pair_info.asset_infos.contains(ask_asset_info),
                ContractError::PoolAssetsMismatch {
                    pool: pool_addr.to_string(),
                    offer_asset: offer_asset_info.to_string(),
                    ask_asset: ask_asset_info.to_string(),
                }
            );

            Ok((pool_addr, offer_asset_info.clone(), ask_asset_info.clone()))
        }
    }
}

/// Creates a message of type [`CosmosMsg`] representing a swap operation.
///
/// The pool's own spread check is disabled (belief price set to the maximum): the route's
/// `minimum_receive`, checked on the final output, is what protects the swapper.
///
/// * **pair_contract** pool contract for which the swap operation is performed.
///
/// * **offer_asset** asset that is swapped. It also mentions the amount to swap.
///
/// * **ask_asset_info** asset that is swapped to.
///
/// * **to** address that receives the ask assets.
pub fn asset_into_swap_msg(
    pair_contract: String,
    offer_asset: Asset,
    ask_asset_info: AssetInfo,
    to: Option<String>,
) -> StdResult<CosmosMsg> {
    let belief_price = Some(Decimal::MAX);
    let max_spread = None;

    match &offer_asset.info {
        AssetInfo::NativeToken { denom } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract,
            funds: vec![Coin {
                denom: denom.to_string(),
                amount: offer_asset.amount,
            }],
            msg: to_json_binary(&PairExecuteMsg::Swap {
                offer_asset: Asset {
                    amount: offer_asset.amount,
                    ..offer_asset
                },
                ask_asset_info: Some(ask_asset_info),
                belief_price,
                max_spread,
                to,
            })?,
        })),
        AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            funds: vec![],
            msg: to_json_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract,
                amount: offer_asset.amount,
                msg: to_json_binary(&astroport::pair::Cw20HookMsg::Swap {
                    ask_asset_info: Some(ask_asset_info),
                    belief_price,
                    max_spread,
                    to,
                })?,
            })?,
        })),
    }
}
