use cosmwasm_std::{
    to_binary, Coin, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    WasmMsg,
};

use crate::querier::compute_tax;
use crate::state::{read_config, Config};

use cw20::Cw20ExecuteMsg;
use terra_cosmwasm::{create_swap_msg, create_swap_send_msg, TerraMsgWrapper};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;
use terraswap::querier::{query_balance, query_pair_info, query_token_balance};
use terraswap::router::SwapOperation;

/// Execute swap operation
/// swap all offer asset to ask asset
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<String>,
) -> StdResult<Response<TerraMsgWrapper>> {
    if env.contract.address != info.sender {
        return Err(StdError::generic_err("unauthorized"));
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
                let amount =
                    amount.checked_sub(compute_tax(deps.as_ref(), amount, offer_denom.clone())?)?;
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
        SwapOperation::TerraSwap {
            offer_asset_info,
            ask_asset_info,
        } => {
            let config: Config = read_config(deps.storage)?;
            let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
            let pair_info: PairInfo = query_pair_info(
                &deps.querier,
                terraswap_factory,
                &[offer_asset_info.clone(), ask_asset_info.clone()],
            )?;

            let amount = match offer_asset_info.clone() {
                AssetInfo::NativeToken { denom } => {
                    query_balance(&deps.querier, env.contract.address, denom.to_string())?
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

    Ok(Response {
        submessages: vec![],
        messages,
        attributes: vec![],
        data: None,
    })
}

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
            let amount = offer_asset.amount.checked_sub(compute_tax(
                deps.as_ref(),
                offer_asset.amount,
                denom.clone(),
            )?)?;
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_contract,
                send: vec![Coin { denom, amount }],
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
            send: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract,
                amount: offer_asset.amount,
                msg: Some(to_binary(&PairExecuteMsg::Swap {
                    offer_asset,
                    belief_price: None,
                    max_spread,
                    to,
                })?),
            })?,
        })),
    }
}
