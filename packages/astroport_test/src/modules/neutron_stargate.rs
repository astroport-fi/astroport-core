use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::{Ok, Result as AnyResult};
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, Api, BankMsg, Binary, BlockInfo, CustomMsg, CustomQuery,
    Empty, Querier, Storage, SubMsgResponse,
};
use cw_multi_test::{
    AppResponse, BankSudo, CosmosRouter, Module, Stargate, StargateMsg, StargateQuery, SudoMsg,
};
use itertools::Itertools;
use neutron_std::types::neutron::dex::{
    LimitOrderTrancheUser, MsgCancelLimitOrder, MsgCancelLimitOrderResponse, MsgPlaceLimitOrder,
    QueryAllLimitOrderTrancheUserByAddressRequest, QueryAllLimitOrderTrancheUserByAddressResponse,
    QuerySimulateCancelLimitOrderRequest, QuerySimulateCancelLimitOrderResponse,
};
use prost::Message;
use sha2::Digest;

use astroport::token_factory::{
    MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint, MsgSetBeforeSendHook,
};

#[derive(Default)]
pub struct NeutronStargate {
    // user -> tranche_key -> limit_order
    orders: RefCell<HashMap<String, HashMap<String, MsgPlaceLimitOrder>>>,
}

impl NeutronStargate {
    const ESCROW_ADDR: &'static str =
        "cosmwasm1ypz7dakxd9umutjtxpk7md3ja5shk84qlj7cv0f6yqkj2naef00q4rdsps";
}

impl Stargate for NeutronStargate {}

impl Module for NeutronStargate {
    type ExecT = StargateMsg;
    type QueryT = StargateQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        let StargateMsg {
            type_url, value, ..
        } = msg;

        match type_url.as_str() {
            MsgCreateDenom::TYPE_URL => {
                let tf_msg: MsgCreateDenom = value.try_into()?;
                let submsg_response = SubMsgResponse {
                    events: vec![],
                    data: Some(
                        MsgCreateDenomResponse {
                            new_token_denom: format!("factory/{sender}/{}", tf_msg.subdenom),
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
                let to_address = tf_msg.mint_to_address.to_string();
                let bank_sudo = BankSudo::Mint {
                    to_address,
                    amount: coins(mint_coins.amount.parse()?, mint_coins.denom),
                };
                router.sudo(api, storage, block, bank_sudo.into())
            }
            MsgBurn::TYPE_URL => {
                let tf_msg: MsgBurn = value.try_into()?;
                let burn_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgBurn!");
                let burn_msg = BankMsg::Burn {
                    amount: coins(burn_coins.amount.parse()?, burn_coins.denom),
                };
                router.execute(
                    api,
                    storage,
                    block,
                    Addr::unchecked(sender),
                    burn_msg.into(),
                )
            }
            MsgSetBeforeSendHook::TYPE_URL => {
                let before_hook_msg: MsgSetBeforeSendHook = value.try_into()?;
                let msg = BankSudo::SetHook {
                    contract_addr: before_hook_msg.cosmwasm_address,
                    denom: before_hook_msg.denom,
                };
                router.sudo(api, storage, block, SudoMsg::Bank(msg))
            }
            MsgPlaceLimitOrder::TYPE_URL => {
                let tranche_key = format!("{:x}", sha2::Sha256::digest(&value));

                // Escrow tokens
                let value: MsgPlaceLimitOrder = value.try_into()?;
                let bank_msg = BankMsg::Send {
                    to_address: Self::ESCROW_ADDR.to_string(),
                    amount: vec![coin(value.amount_in.parse().unwrap(), &value.token_in)],
                };
                router.execute(api, storage, block, sender.clone(), bank_msg.into())?;

                self.orders
                    .borrow_mut()
                    .entry(sender.to_string())
                    .or_default()
                    .insert(tranche_key, value);
                Ok(AppResponse::default())
            }
            MsgCancelLimitOrder::TYPE_URL => {
                let cancel_msg: MsgCancelLimitOrder = value.try_into()?;
                let order = self
                    .orders
                    .borrow_mut()
                    .get_mut(&sender.to_string())
                    .and_then(|m| m.remove(&cancel_msg.tranche_key))
                    .ok_or_else(|| anyhow::anyhow!("Order not found"))?;

                // Unescrow tokens
                let msg = BankMsg::Send {
                    to_address: sender.to_string(),
                    amount: vec![coin(order.amount_in.parse().unwrap(), &order.token_in)],
                };
                router.execute(
                    api,
                    storage,
                    block,
                    Addr::unchecked(Self::ESCROW_ADDR),
                    msg.into(),
                )
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
        request: Self::QueryT,
    ) -> AnyResult<Binary> {
        match request.path.as_str() {
            "/neutron.dex.Query/LimitOrderTrancheUserAllByAddress" => {
                let request =
                    QueryAllLimitOrderTrancheUserByAddressRequest::decode(request.data.as_slice())
                        .unwrap();

                Ok(to_json_binary(
                    &QueryAllLimitOrderTrancheUserByAddressResponse {
                        limit_orders: self
                            .orders
                            .borrow()
                            .get(&request.address)
                            .map(|m| {
                                m.iter()
                                    .map(|(tranche_key, order)| LimitOrderTrancheUser {
                                        trade_pair_id: None,
                                        tick_index_taker_to_maker: 0,
                                        tranche_key: tranche_key.clone(),
                                        address: order.receiver.clone(),
                                        shares_owned: "".to_string(),
                                        shares_withdrawn: "".to_string(),
                                        shares_cancelled: "".to_string(),
                                        order_type: order.order_type,
                                    })
                                    .collect_vec()
                            })
                            .unwrap_or_default(),
                        pagination: None,
                    },
                )?)
            }
            "/neutron.dex.Query/SimulateCancelLimitOrder" => {
                let request = QuerySimulateCancelLimitOrderRequest::decode(request.data.as_slice())
                    .unwrap()
                    .msg
                    .unwrap();

                let order = self
                    .orders
                    .borrow()
                    .get(&request.creator)
                    .unwrap()
                    .get(&request.tranche_key)
                    .cloned()
                    .unwrap();
                let resp = Some(MsgCancelLimitOrderResponse {
                    taker_coin_out: None,
                    maker_coin_out: Some(
                        coin(order.amount_in.parse().unwrap(), &order.token_in).into(),
                    ),
                });

                Ok(to_json_binary(&QuerySimulateCancelLimitOrderResponse {
                    resp,
                })?)
            }
            _ => Err(anyhow::anyhow!(
                "Unexpected stargate query request {}",
                request.path
            )),
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        unimplemented!("Sudo not implemented")
    }
}
