use crate::error::ContractError;
use crate::state::{CONFIG, PROVIDE_TMP_DATA, WITHDRAW_TMP_DATA};
use crate::utils::{ensure_min_assets_to_receive, mint_liquidity_token_message};
use astroport::token_factory::MsgCreateDenomResponse;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, ensure, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsgResponse, SubMsgResult,
};
use itertools::Itertools;

/// A `reply` call code ID used for sub-messages.
#[cw_serde]
pub enum ReplyIds {
    CreateDenom = 1,
    PostProvide = 2,
    PostWithdraw = 3,
}

impl TryFrom<u64> for ReplyIds {
    type Error = StdError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ReplyIds::CreateDenom),
            2 => Ok(ReplyIds::PostProvide),
            3 => Ok(ReplyIds::PostWithdraw),
            _ => Err(StdError::ParseErr {
                target_type: "ReplyIds".to_string(),
                msg: "Failed to parse reply".to_string(),
            }),
        }
    }
}

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
        ReplyIds::PostProvide => {
            let provide_data = PROVIDE_TMP_DATA.load(deps.storage)?;
            let config = CONFIG.load(deps.storage)?;

            let lp_tokens = deps
                .querier
                .query_balance(&env.contract.address, &config.vault_lp_denom)?
                .amount
                - provide_data.lp_tokens_before;

            if let Some(min_lp_to_receive) = provide_data.min_lp_to_receive {
                ensure!(
                    lp_tokens >= min_lp_to_receive,
                    ContractError::ProvideSlippageViolation(lp_tokens, min_lp_to_receive)
                );
            }

            let msgs = mint_liquidity_token_message(
                deps.querier,
                &config,
                &env.contract.address,
                &provide_data.receiver,
                lp_tokens,
                provide_data.auto_stake,
            )?;

            Ok(Response::default().add_messages(msgs).add_attributes([
                attr("action", "provide_liquidity"),
                attr("receiver", provide_data.receiver),
                attr("assets", provide_data.assets.iter().join(", ")),
                attr("share", lp_tokens),
            ]))
        }
        ReplyIds::PostWithdraw => {
            let withdraw_data = WITHDRAW_TMP_DATA.load(deps.storage)?;

            let config = CONFIG.load(deps.storage)?;

            let refund_assets = config
                .pair_info
                .query_pools(&deps.querier, env.contract.address)?;

            ensure_min_assets_to_receive(
                &config.pair_info,
                &refund_assets,
                withdraw_data.min_assets_to_receive,
            )?;

            let messages = refund_assets
                .clone()
                .into_iter()
                .map(|asset| asset.into_msg(&withdraw_data.receiver))
                .collect::<StdResult<Vec<_>>>()?;

            Ok(Response::new().add_messages(messages).add_attributes([
                attr("action", "withdraw_liquidity"),
                attr("withdrawn_share", withdraw_data.lp_amount),
                attr("refund_assets", refund_assets.iter().join(", ")),
            ]))
        }
    }
}
