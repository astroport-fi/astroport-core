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
use terra_cosmwasm::{create_swap_msg, create_swap_send_msg, TerraMsgWrapper};

/// ## Description
/// Execute swap operation. Swap all offer asset to ask asset.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the
/// specified attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **operation** is the object of type [`SwapOperation`].
///
/// * **to** is the object of type [`Option<String>`].
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<String>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let messages: Vec<CosmosMsg<TerraMsgWrapper>> = match operation {
        SwapOperation::NativeSwap {
            offer_denom,
            ask_denom,
        } => {
            let amount =
                query_balance(&deps.querier, env.contract.address, offer_denom.to_string())?;
            if let Some(to) = to {
                // if the opeation is last, and requires send
                // deduct tax from the offer_coin
                let asset = Asset {
                    info: AssetInfo::NativeToken {
                        denom: offer_denom.clone(),
                    },
                    amount,
                };
                let amount = amount.checked_sub(asset.compute_tax(&deps.querier)?)?;
                vec![create_swap_send_msg(
                    to,
                    Coin {
                        denom: offer_denom,
                        amount,
                    },
                    ask_denom,
                )]
            } else {
                vec![create_swap_msg(
                    Coin {
                        denom: offer_denom,
                        amount,
                    },
                    ask_denom,
                )]
            }
        }
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
                None,
                to,
            )?]
        }
    };

    Ok(Response::new().add_messages(messages))
}

/// ## Description
/// Creates a message with an exchange operation of type CosmosMsg for each asset.
/// Returns the [`CosmosMsg<TerraMsgWrapper>`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **pair_contract** is the object of type [`String`].
///
/// * **offer_asset** is the object of type [`Asset`].
///
/// * **max_spread** is the object of type [`Option<Decimal>`].
///
/// * **to** is the object of type [`Option<String>`].
pub fn asset_into_swap_msg(
    deps: DepsMut,
    pair_contract: String,
    offer_asset: Asset,
    max_spread: Option<Decimal>,
    to: Option<String>,
) -> StdResult<CosmosMsg<TerraMsgWrapper>> {
    match offer_asset.info.clone() {
        AssetInfo::NativeToken { denom } => {
            // deduct tax first
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
                    belief_price: None,
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
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset,
                    belief_price: None,
                    max_spread,
                    to,
                })?,
            })?,
        })),
    }
}
