use cosmwasm_std::{DepsMut, Env, Reply, Response, StdError, SubMsgResponse, SubMsgResult};
use itertools::Itertools;

use astroport::pair_concentrated_duality::ReplyIds;
use astroport::token_factory::MsgCreateDenomResponse;
use astroport_pcl_common::state::Precisions;

use crate::error::ContractError;
use crate::orderbook::execute::process_cumulative_trade;
use crate::orderbook::state::OrderbookState;
use crate::orderbook::utils::{fetch_cumulative_trade, Liquidity};
use crate::state::CONFIG;

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match ReplyIds::try_from(msg.id)? {
        ReplyIds::CreateDenom => {
            if let SubMsgResult::Ok(SubMsgResponse { data: Some(b), .. }) = msg.result {
                let MsgCreateDenomResponse { new_token_denom } = b.try_into()?;

                CONFIG.update(deps.storage, |mut config| {
                    if !config.pair_info.liquidity_token.is_empty() {
                        return Err(StdError::generic_err(
                            "Liquidity token is already set in the config",
                        ));
                    }

                    config
                        .pair_info
                        .liquidity_token
                        .clone_from(&new_token_denom);

                    Ok(config)
                })?;

                Ok(Response::new().add_attribute("lp_denom", new_token_denom))
            } else {
                Err(ContractError::FailedToParseReply {})
            }
        }
        ReplyIds::PostLimitOrderCb => {
            let mut ob_state = OrderbookState::load(deps.storage)?;
            ob_state.fetch_all_orders(deps.as_ref(), &env.contract.address)?;

            let mut config = CONFIG.load(deps.storage)?;
            let liquidity = Liquidity::new(deps.querier, &config, &ob_state, true)?;

            // We need to process cumulative trade only if we have orders number less than expected.
            // It means that they were auto-executed,
            // and we need to send maker fees and possibly repeg PCL.
            // We don't need
            // to process partially filled orders
            // as their traces stay on chain until the next contract execution.
            let response = if ob_state.orders.len() < (ob_state.orders_number * 2) as usize {
                let precisions = Precisions::new(deps.storage)?;
                // This call fetches cumulative trade based on diff between pre-reply and current total balances.
                // The fact that current orders number less than expected must guarantee
                // there is cumulative trade to process.
                let cumulative_trade = fetch_cumulative_trade(
                    &precisions,
                    &ob_state.pre_reply_balances,
                    &liquidity.total(),
                )?
                .ok_or_else(|| StdError::generic_err("PCL pool lost its liquidity in orderbook"))?;

                // Process all filled orders as one cumulative trade; send maker fees; repeg PCL
                let mut pools = liquidity.total_dec(&precisions)?;
                let mut balances = pools
                    .iter_mut()
                    .map(|asset| &mut asset.amount)
                    .collect_vec();

                // TODO: ideally we should save all PCL profit tracking logic and possible repegging changes.
                // However, if we did so we would need to cancel all orders and
                // post new ones. Then, of course, we have to call reply logic again to snapshot new
                // order tranche keys and process possible filled order.
                // This might end up in costly and possibly endless recursion.
                //
                // For now we only send maker fees and pretend that this
                // cumulative trade has never happened for PCL.

                process_cumulative_trade(
                    deps.as_ref(),
                    &env,
                    &cumulative_trade,
                    &mut config,
                    &mut balances,
                    &precisions,
                    None,
                )?
            } else {
                Response::default()
            };

            ob_state.last_balances = liquidity.orderbook;
            ob_state.save(deps.storage)?;

            Ok(response.add_attribute("action", "post_limit_order_callback"))
        }
    }
}
