use cosmwasm_std::{
    to_binary, Coin, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Response, StdResult, WasmMsg,
};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use astroport::querier::{query_balance, query_pair_info, query_token_balance};
use astroport::router::SwapOperation;
use cw20::Cw20ExecuteMsg;

/// ## Description
/// Execute a swap operation. Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the
/// specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **operation** is an object of type [`SwapOperation`]. It's the swap operation to perform (offer/ask assets and the offer asset amount).
///
/// * **to** is an object of type [`Option<String>`]. This is the address that receives the ask assets.
///
/// * **single** defines whether this swap is single or part of a multi hop route.
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<String>,
    max_spread: Option<Decimal>,
    single: bool,
) -> Result<Response, ContractError> {
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let messages: Vec<CosmosMsg> = match operation {
        SwapOperation::AstroSwap {
            offer_asset_info,
            ask_asset_info,
        } => {
            let config: Config = CONFIG.load(deps.storage)?;
            let astroport_factory = config.astroport_factory;
            let pair_info: PairInfo = query_pair_info(
                &deps.querier,
                astroport_factory,
                &[offer_asset_info.clone(), ask_asset_info],
            )?;

            let amount = match offer_asset_info.clone() {
                AssetInfo::NativeToken { denom } => {
                    query_balance(&deps.querier, env.contract.address, denom)?
                }
                AssetInfo::Token { contract_addr } => {
                    query_token_balance(&deps.querier, contract_addr, env.contract.address)?
                }
            };
            let offer_asset: Asset = Asset {
                info: offer_asset_info,
                amount,
            };

            vec![asset_into_swap_msg(
                deps,
                pair_info.contract_addr.to_string(),
                offer_asset,
                max_spread,
                to,
                single,
            )?]
        }
        SwapOperation::NativeSwap { .. } => return Err(ContractError::NativeSwapNotSupported {}),
    };

    Ok(Response::new().add_messages(messages))
}

/// ## Description
/// Creates a message with an exchange operation of type CosmosMsg for each asset.
/// Returns the [`CosmosMsg`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **pair_contract** is an object of type [`String`]. This is the Astroport pair contract for which the swap operation is performed.
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset that is swapped. It also mentions the amount to swap.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. This is the max spread enforced for the swap.
///
/// * **to** is an object of type [`Option<String>`]. This is the address that receives the ask assets.
///
/// * **single** defines whether this swap is single or part of a multi hop route.
pub fn asset_into_swap_msg(
    deps: DepsMut,
    pair_contract: String,
    offer_asset: Asset,
    max_spread: Option<Decimal>,
    to: Option<String>,
    single: bool,
) -> StdResult<CosmosMsg> {
    // Disabling spread assertion if this swap is part of a multi hop route
    let belief_price = if single { None } else { Some(Decimal::MAX) };

    match offer_asset.info.clone() {
        AssetInfo::NativeToken { denom } => {
            // Deduct tax first
            let amount = offer_asset
                .amount
                .checked_sub(offer_asset.compute_tax(&deps.querier)?)?;
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_contract,
                funds: vec![Coin { denom, amount }],
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset: Asset {
                        amount,
                        ..offer_asset
                    },
                    belief_price,
                    max_spread,
                    to,
                })?,
            }))
        }
        AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract,
                amount: offer_asset.amount,
                msg: to_binary(&astroport::pair::Cw20HookMsg::Swap {
                    belief_price,
                    max_spread,
                    to,
                })?,
            })?,
        })),
    }
}
