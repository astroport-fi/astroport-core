use cosmwasm_std::{DepsMut, Env, Reply, Response, StdError, SubMsgResponse, SubMsgResult};

use astroport::pair_concentrated_duality::ReplyIds;
use astroport::token_factory::MsgCreateDenomResponse;

use crate::error::ContractError;
use crate::orderbook::state::OrderbookState;
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
            // Query total liquidity sitting on orderbook and cache it in the contract state
            let mut ob_state = OrderbookState::load(deps.storage)?;
            ob_state.fetch_all_orders(deps.as_ref(), &env.contract.address)?;
            ob_state.last_balances =
                ob_state.query_ob_liquidity(deps.as_ref(), &env.contract.address, true)?;
            ob_state.save(deps.storage)?;

            Ok(Response::default().add_attribute("action", "post_limit_order_callback"))
        }
    }
}
