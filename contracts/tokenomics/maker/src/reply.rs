use crate::error::ContractError;
use crate::state::{CONFIG, TMP_REPLY_DATA};
use crate::utils::build_swap_msg;
use astroport::asset::{Asset, AssetInfoExt};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, wasm_execute, BankMsg, DepsMut, Empty, Env, Reply, Response, SubMsg,
};

pub const POST_COLLECT_REPLY_ID: u64 = 1;
pub const POST_DEV_FUND_SWAP_REPLY_ID: u64 = 2;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        POST_COLLECT_REPLY_ID => {
            let config = CONFIG.load(deps.storage)?;
            let mut astro_balance = deps
                .querier
                .query_balance(&env.contract.address, &config.astro_denom)?;

            let mut attrs = vec![
                attr("action", "post_collect_reply"),
                attr("total_astro", astro_balance.to_string()),
            ];

            let msgs: Vec<SubMsg> = if config.astro_denom.to_lowercase().starts_with("ibc/") {
                // This is an outpost chain
                vec![SubMsg::new(wasm_execute(
                    config.collector,
                    // Satellite type parameter is only needed for CheckMessages endpoint which is not used in Maker contract.
                    // So it's safe to pass Empty as CustomMsg
                    &astro_satellite_package::ExecuteMsg::<Empty>::TransferAstro {},
                    vec![astro_balance.clone()],
                )?)]
            } else {
                // This is a hub chain
                let mut msgs = vec![];

                if let Some(dev_fund_conf) = config.dev_fund_conf {
                    let dev_share = astro_balance.amount * dev_fund_conf.share;
                    astro_balance.amount -= dev_share;

                    if !dev_share.is_zero() {
                        // Snapshot pre-reply asset balance
                        let amount = dev_fund_conf
                            .asset_info
                            .query_pool(&deps.querier, env.contract.address)?;
                        TMP_REPLY_DATA.save(deps.storage, &amount)?;

                        // Swap ASTRO and process result in reply
                        msgs.push(SubMsg::reply_on_success(
                            build_swap_msg(
                                &Asset::native(&config.astro_denom, dev_share),
                                &dev_fund_conf.asset_info,
                                config.max_spread,
                                &dev_fund_conf.pool_addr,
                            )?,
                            POST_DEV_FUND_SWAP_REPLY_ID,
                        ));

                        attrs.push(attr(
                            "astro_to_dev_fund",
                            coin(dev_share.u128(), &config.astro_denom).to_string(),
                        ));
                    }
                };

                msgs.push(SubMsg::new(BankMsg::Send {
                    to_address: config.collector.to_string(),
                    amount: vec![astro_balance.clone()],
                }));

                msgs
            };

            attrs.push(attr("astro_to_staking", astro_balance.to_string()));

            Ok(Response::new().add_submessages(msgs).add_attributes(attrs))
        }
        POST_DEV_FUND_SWAP_REPLY_ID => {
            let config = CONFIG.load(deps.storage)?;
            let dev_fund_conf = config.dev_fund_conf.expect("Dev fund config must be set");

            let cur_amount = dev_fund_conf
                .asset_info
                .query_pool(&deps.querier, env.contract.address)?;
            let pre_reply_amount = TMP_REPLY_DATA.load(deps.storage)?;
            let dev_fee = dev_fund_conf
                .asset_info
                .with_balance(cur_amount - pre_reply_amount);

            Ok(Response::new()
                .add_attributes([
                    attr("action", "process_dev_fund"),
                    attr("dev_fund_amount", dev_fee.to_string()),
                ])
                .add_message(dev_fee.into_msg(&dev_fund_conf.address)?))
        }
        _ => Err(ContractError::InvalidReplyId {}),
    }
}
