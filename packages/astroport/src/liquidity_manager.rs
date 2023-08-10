use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

use crate::asset::Asset;
use crate::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg};

#[cw_serde]
pub struct InstantiateMsg {
    pub astroport_factory: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ProvideLiquidity {
        pair_addr: String,
        pair_msg: PairExecuteMsg,
        min_lp_to_receive: Option<Uint128>,
    },
    Receive(Cw20ReceiveMsg),
}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    WithdrawLiquidity {
        pair_msg: PairCw20HookMsg,
        #[serde(default)]
        min_assets_to_receive: Vec<Asset>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Uint128)]
    SimulateProvide {
        pair_addr: String,
        pair_msg: PairExecuteMsg,
    },
    #[returns(Vec<Asset>)]
    SimulateWithdraw {
        pair_addr: String,
        lp_tokens: Uint128,
    },
}
