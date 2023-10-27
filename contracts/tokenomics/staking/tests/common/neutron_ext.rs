use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use anyhow::Result as AnyResult;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coins, Addr, Api, Binary, BlockInfo, CustomQuery, Empty, Querier, Storage, SubMsgResponse,
};
use cw_multi_test::{
    AppResponse, BankSudo, CosmosRouter, Module, Stargate, StargateMsg, StargateQuery,
};

use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgCreateDenomResponse, MsgMint,
};

#[derive(Default)]
pub struct NeutronStargate {
    pub cw_pools: RefCell<HashMap<u64, String>>,
}

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
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg.type_url.as_str() {
            MsgCreateDenom::TYPE_URL => {
                let tf_msg: MsgCreateDenom = msg.value.try_into()?;
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
                let tf_msg: MsgMint = msg.value.try_into()?;
                let mint_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgMint!");
                let bank_sudo = BankSudo::Mint {
                    to_address: tf_msg.mint_to_address,
                    amount: coins(mint_coins.amount.parse()?, mint_coins.denom),
                };
                router.sudo(api, storage, block, bank_sudo.into())
            }
            MsgBurn::TYPE_URL => {
                let tf_msg: MsgBurn = msg.value.try_into()?;
                let burn_coins = tf_msg
                    .amount
                    .expect("Empty amount in tokenfactory MsgBurn!");
                let bank_sudo = BankSudo::Burn {
                    from_address: tf_msg.sender,
                    amount: coins(burn_coins.amount.parse()?, burn_coins.denom),
                };
                router.sudo(api, storage, block, bank_sudo.into())
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unexpected exec msg {msg:?} from {sender:?}",
                ))
            }
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
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        unimplemented!("sudo for Neutron Stargate mock module is not implemented")
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
            _ => {
                return Err(anyhow::anyhow!(
                    "Unexpected stargate query request {request:?}",
                ))
            }
        }
    }
}

impl Stargate for NeutronStargate {}
