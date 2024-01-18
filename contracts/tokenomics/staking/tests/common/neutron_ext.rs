use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use anyhow::Result as AnyResult;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coin, to_json_binary, Addr, Api, BankMsg, Binary, BlockInfo, CustomQuery, Querier, Storage,
    SubMsgResponse,
};
use cw_multi_test::{AppResponse, BankSudo, CosmosRouter, Stargate, WasmSudo};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint, MsgSetBeforeSendHook,
    MsgSetDenomMetadata,
};

use astroport::tokenfactory_tracker::SudoMsg;

pub const TOKEN_FACTORY_MODULE: &str = "wasm1tokenfactory";

pub struct NeutronStargate {
    // key: token denom, value: hook contract address
    pub before_send_hooks: RefCell<HashMap<String, String>>,
}

impl NeutronStargate {
    pub fn new() -> Self {
        Self {
            before_send_hooks: Default::default(),
        }
    }
}

impl Stargate for NeutronStargate {
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
                let mint_resp = router.sudo(api, storage, block, bank_sudo.into())?;

                if let Some(hook_contract) = self.before_send_hooks.borrow().get(&cw_coin.denom) {
                    // Call tracker contract to update the balance
                    let wasm_sudo = WasmSudo {
                        contract_addr: Addr::unchecked(hook_contract),
                        msg: to_json_binary(&SudoMsg::BlockBeforeSend {
                            from: TOKEN_FACTORY_MODULE.to_string(),
                            to: tf_msg.mint_to_address,
                            amount: cw_coin,
                        })?,
                    };
                    router.sudo(api, storage, block, wasm_sudo.into())
                } else {
                    Ok(mint_resp)
                }
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
                let burn_resp = router.execute(
                    api,
                    storage,
                    block,
                    Addr::unchecked(TOKEN_FACTORY_MODULE),
                    burn_msg.into(),
                )?;

                if let Some(hook_contract) = self.before_send_hooks.borrow().get(&cw_coin.denom) {
                    // Call tracker contract to update the balance
                    let wasm_sudo = WasmSudo {
                        contract_addr: Addr::unchecked(hook_contract),
                        msg: to_json_binary(&SudoMsg::BlockBeforeSend {
                            from: "".to_string(), // on real chain this is likely set to denom admin but tracker doesn't care
                            to: TOKEN_FACTORY_MODULE.to_string(),
                            amount: cw_coin,
                        })?,
                    };
                    router.sudo(api, storage, block, wasm_sudo.into())
                } else {
                    Ok(burn_resp)
                }
            }
            MsgSetDenomMetadata::TYPE_URL => {
                // TODO: Implement this if needed
                Ok(AppResponse::default())
            }
            MsgSetBeforeSendHook::TYPE_URL => {
                let tf_msg: MsgSetBeforeSendHook = value.try_into()?;
                self.before_send_hooks
                    .borrow_mut()
                    .insert(tf_msg.denom, tf_msg.cosmwasm_address);

                Ok(AppResponse::default())
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
