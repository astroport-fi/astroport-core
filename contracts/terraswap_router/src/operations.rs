use cosmwasm_std::{
    to_binary, Api, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, HandleResult, HumanAddr,
    Querier, StdError, StdResult, Storage, WasmMsg,
};

use crate::querier::compute_tax;
use crate::state::{read_config, Config};

use cw20::Cw20HandleMsg;
use terra_cosmwasm::{create_swap_msg, create_swap_send_msg, TerraMsgWrapper};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::HandleMsg as PairHandleMsg;
use terraswap::querier::{query_balance, query_pair_info, query_token_balance};
use terraswap::router::SwapOperation;

/// Execute swap operation
/// swap all offer asset to ask asset
pub fn execute_swap_operation<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    operation: SwapOperation,
    to: Option<HumanAddr>,
) -> HandleResult<TerraMsgWrapper> {
    if env.contract.address != env.message.sender {
        return Err(StdError::unauthorized());
    }

    let messages: Vec<CosmosMsg<TerraMsgWrapper>> = match operation {
        SwapOperation::NativeSwap {
            offer_denom,
            ask_denom,
        } => {
            let amount = query_balance(&deps, &env.contract.address, offer_denom.to_string())?;
            if let Some(to) = to {
                // if the opeation is last, and requires send
                // deduct tax from the offer_coin
                let amount = (amount - compute_tax(&deps, amount, offer_denom.clone())?)?;
                vec![create_swap_send_msg(
                    env.contract.address,
                    to,
                    Coin {
                        denom: offer_denom,
                        amount,
                    },
                    ask_denom,
                )]
            } else {
                vec![create_swap_msg(
                    env.contract.address,
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
            let config: Config = read_config(&deps.storage)?;
            let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;
            let pair_info: PairInfo = query_pair_info(
                &deps,
                &terraswap_factory,
                &[offer_asset_info.clone(), ask_asset_info.clone()],
            )?;

            let amount = match offer_asset_info.clone() {
                AssetInfo::NativeToken { denom } => {
                    query_balance(&deps, &env.contract.address, denom.to_string())?
                }
                AssetInfo::Token { contract_addr } => {
                    query_token_balance(&deps, &contract_addr, &env.contract.address)?
                }
            };
            let offer_asset: Asset = Asset {
                info: offer_asset_info,
                amount,
            };

            vec![asset_into_swap_msg(
                &deps,
                pair_info.contract_addr,
                offer_asset,
                None,
                to,
            )?]
        }
    };

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

pub fn asset_into_swap_msg<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pair_contract: HumanAddr,
    offer_asset: Asset,
    max_spread: Option<Decimal>,
    to: Option<HumanAddr>,
) -> StdResult<CosmosMsg<TerraMsgWrapper>> {
    match offer_asset.info.clone() {
        AssetInfo::NativeToken { denom } => {
            // deduct tax first
            let amount =
                (offer_asset.amount - compute_tax(&deps, offer_asset.amount, denom.clone())?)?;
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_contract,
                send: vec![Coin { denom, amount }],
                msg: to_binary(&PairHandleMsg::Swap {
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
            contract_addr,
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: pair_contract,
                amount: offer_asset.amount,
                msg: Some(to_binary(&PairHandleMsg::Swap {
                    offer_asset,
                    belief_price: None,
                    max_spread,
                    to,
                })?),
            })?,
        })),
    }
}
