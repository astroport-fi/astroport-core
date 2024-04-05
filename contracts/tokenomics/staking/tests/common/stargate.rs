use std::fmt::Debug;

use anyhow::Result as AnyResult;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coin, Addr, Api, BankMsg, Binary, BlockInfo, CustomQuery, Querier, Storage, SubMsgResponse,
};
use cw_multi_test::{AppResponse, BankSudo, CosmosRouter, Stargate};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint, MsgSetBeforeSendHook,
    MsgSetDenomMetadata,
};

#[derive(Default)]
pub struct StargateKeeper {}

impl Stargate for StargateKeeper {
    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        type_url: String,
        value: Binary,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match type_url.as_str() {
            MsgCreateDenom::TYPE_URL => {
                let tf_msg: MsgCreateDenom = value.try_into()?;
                let submsg_response = SubMsgResponse {
                    events: vec![],
                    data: Some(
                        MsgCreateDenomResponse {
                            new_token_denom: format!(
                                "factory/{}/{}",
                                tf_msg.sender, tf_msg.subdenom
                            ),
                        }
                        .into(),
                    ),
                };
                Ok(submsg_response.into())
            }
            MsgMint::TYPE_URL => {
                let tf_msg: MsgMint = value.try_into()?;
                let mint_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgMint!");
                let cw_coin = coin(mint_coins.amount.parse()?, mint_coins.denom);
                let bank_sudo = BankSudo::Mint {
                    to_address: tf_msg.mint_to_address.clone(),
                    amount: vec![cw_coin.clone()],
                };

                router.sudo(api, storage, block, bank_sudo.into())
            }
            MsgBurn::TYPE_URL => {
                let tf_msg: MsgBurn = value.try_into()?;
                let burn_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgBurn!");
                let cw_coin = coin(burn_coins.amount.parse()?, burn_coins.denom);
                let burn_msg = BankMsg::Burn {
                    amount: vec![cw_coin.clone()],
                };

                router.execute(
                    api,
                    storage,
                    block,
                    Addr::unchecked(&tf_msg.sender),
                    burn_msg.into(),
                )
            }
            MsgSetDenomMetadata::TYPE_URL => {
                // TODO: Implement this if needed
                Ok(AppResponse::default())
            }
            MsgSetBeforeSendHook::TYPE_URL => {
                let tf_msg: MsgSetBeforeSendHook = value.try_into()?;

                let bank_sudo = BankSudo::SetHook {
                    denom: tf_msg.denom,
                    contract_addr: tf_msg.cosmwasm_address,
                };

                router.sudo(api, storage, block, bank_sudo.into())
            }
            _ => Err(anyhow::anyhow!(
                "Unexpected exec msg {type_url} from {sender:?}",
            )),
        }
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        _path: String,
        _data: Binary,
    ) -> AnyResult<Binary> {
        unimplemented!("Stargate queries are not implemented")
        // match path.as_str() {
        //     "/osmosis.poolmanager.v1beta1.Query/Params" => {
        //         Ok(to_json_binary(&poolmanager::v1beta1::ParamsResponse {
        //             params: Some(poolmanager::v1beta1::Params {
        //                 pool_creation_fee: vec![coin(1000_000000, "uosmo").into()],
        //                 taker_fee_params: None,
        //                 authorized_quote_denoms: vec![],
        //             }),
        //         })?)
        //     }
        //     _ => Err(anyhow::anyhow!("Unexpected stargate query request {path}")),
        // }
    }
}
