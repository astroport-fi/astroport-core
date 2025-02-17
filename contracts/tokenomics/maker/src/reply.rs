use crate::state::CONFIG;
use astroport::asset::AssetInfoExt;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, DepsMut, Env, Reply, Response, StdError, StdResult};

pub const PROCESS_DEV_FUND_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    match msg.id {
        PROCESS_DEV_FUND_REPLY_ID => {
            let config = CONFIG.load(deps.storage)?;
            let dev_fund_conf = config.dev_fund_conf.expect("Dev fund config must be set");

            let amount = dev_fund_conf
                .asset_info
                .query_pool(&deps.querier, env.contract.address)?;
            let dev_fee = dev_fund_conf.asset_info.with_balance(amount);

            Ok(Response::new()
                .add_attributes([
                    attr("action", "process_dev_fund"),
                    attr("amount", dev_fee.to_string()),
                ])
                .add_message(dev_fee.into_msg(&dev_fund_conf.address)?))
        }
        _ => Err(StdError::generic_err("Invalid reply id")),
    }
}
