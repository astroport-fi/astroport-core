#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, wasm_execute, DepsMut, Empty, Env, Reply, Response, SubMsg};

use crate::error::ContractError;
use crate::state::CONFIG;

pub const POST_COLLECT_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        POST_COLLECT_REPLY_ID => {
            let config = CONFIG.load(deps.storage)?;
            let astro_balance = deps
                .querier
                .query_balance(env.contract.address, &config.astro_denom)?;

            let mut response = Response::new().add_attributes([
                attr("action", "post_collect_reply"),
                attr("astro", astro_balance.to_string()),
            ]);

            let transfer_msg = wasm_execute(
                config.collector,
                // Satellite type parameter is only needed for CheckMessages endpoint which is not used in Maker contract.
                // So it's safe to pass Empty as CustomMsg
                &astro_satellite_package::ExecuteMsg::<Empty>::TransferAstro {},
                vec![astro_balance],
            )?;

            response.messages.push(SubMsg::new(transfer_msg));

            Ok(response)
        }
        _ => Err(ContractError::InvalidReplyId {}),
    }
}
