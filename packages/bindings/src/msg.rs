use cosmwasm_schema::{cw_serde};
use cosmwasm_std::{Coin, CosmosMsg, CustomMsg};

/// A number of Custom messages that can call into the Terra bindings
#[cw_serde]
pub enum TerraMsg {
    // swap
    Swap {
        offer_coin: Coin,
        ask_denom: String
    },
    // swap send
    SwapSend {   
        to_address: String,
        offer_coin: Coin,
        ask_denom: String
    }
}

impl TerraMsg {

    // create swap msg
    pub fn create_swap_msg(offer_coin: Coin, ask_denom: String) -> Self {
        TerraMsg::Swap {
            offer_coin,
            ask_denom,
        }
    }

    // create swap send msg
    pub fn create_swap_send_msg(to_address: String, offer_coin: Coin, ask_denom: String) -> Self {
        TerraMsg::SwapSend {
            to_address,
            offer_coin,
            ask_denom,
        }
    }
}

impl From<TerraMsg> for CosmosMsg<TerraMsg> {
    fn from(msg: TerraMsg) -> CosmosMsg<TerraMsg> {
        CosmosMsg::Custom(msg)
    }
}

impl CustomMsg for TerraMsg {}
