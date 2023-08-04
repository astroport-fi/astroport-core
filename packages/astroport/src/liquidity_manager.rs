use crate::asset::Asset;
use crate::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

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
/// Json representation should be one of the following:
/// 1.
/// ```json
/// {
///   "simulate": {
///     "pair_addr": "wasm1...addr",
///     "pair_msg": {
///       "provide_liquidity": {
///         "assets": [
///          {
///             "info": {
///               "native_token": {
///                 "denom": "uusd"
///               }
///             },
///             "amount": "100000"
///           },
///           {
///             "info": {
///               "token": {
///                 "contract_addr": "wasm1...cw20address"
///               }
///            },
///             "amount": "100000"
///           }
///         ],
///         "slippage_tolerance": "0.02",
///         "auto_stake": true,
///         "receiver": "wasm1...addr"
///       }
///     }
///   }
/// }
/// ```
///
/// 2.
/// ```json
/// {
///   "simulate": {
///     "pair_addr": "wasm1...addr",
///     "pair_msg": {
///       "lp_tokens": "1000"
///     }
///   }
/// }
/// ```
pub enum QueryMsg {
    Simulate {
        pair_addr: String,
        pair_msg: SimulateMessage,
    },
}

#[cw_serde]
#[serde(untagged)]
pub enum SimulateMessage {
    Provide(PairExecuteMsg),
    Withdraw { lp_tokens: Uint128 },
}
