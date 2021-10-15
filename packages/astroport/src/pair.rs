use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetInfo};
use crate::hook::InitHook;

use crate::factory::{factory_config, PairType};
use crate::generator::ExecuteMsg as GeneratorExecuteMsg;
use cosmwasm_std::{
    to_binary, Addr, Decimal, DepsMut, ReplyOn, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Asset infos
    pub asset_infos: [AssetInfo; 2],
    /// Token contract code id for initialization
    pub token_code_id: u64,
    /// Hook for post initialization
    pub init_hook: Option<InitHook>,
    /// Factory contract address
    pub factory_addr: Addr,
    /// Pair type
    pub pair_type: PairType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    /// Post initialize step to allow user to set controlled contract address after creating it
    PostInitialize {},
    /// ProvideLiquidity a user provides pool liquidity
    ProvideLiquidity {
        assets: [Asset; 2],
        slippage_tolerance: Option<Decimal>,
        auto_stack: Option<bool>,
    },
    /// Swap an offer asset to the other
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    UpdateConfig {
        amp: Option<u64>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Sell a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    WithdrawLiquidity {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Pair {},
    Pool {},
    Share { amount: Uint128 },
    Simulation { offer_asset: Asset },
    ReverseSimulation { ask_asset: Asset },
    CumulativePrices {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
}

/// SimulationResponse returns swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulationResponse {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// ReverseSimulationResponse returns reverse swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReverseSimulationResponse {
    pub offer_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CumulativePricesResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
    pub price0_cumulative_last: Uint128,
    pub price1_cumulative_last: Uint128,
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

pub fn generator_address(
    auto_stack: bool,
    factory_addr: Addr,
    deps: &DepsMut,
) -> StdResult<Option<Addr>> {
    Ok(if auto_stack {
        let factory_config = factory_config(factory_addr, deps)?;
        Some(factory_config.generator_address)
    } else {
        None
    })
}

/// Mint LP token to sender or auto deposit into generator if set
pub fn mint_liquidity_token_message(
    pair: Addr,
    lp_token: Addr,
    beneficiary: Addr,
    amount: Uint128,
    generator: Option<Addr>,
) -> StdResult<Vec<SubMsg>> {
    let recipient = if generator.is_some() {
        pair.to_string()
    } else {
        beneficiary.to_string()
    };
    let mut messages: Vec<SubMsg> = vec![SubMsg {
        msg: WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint { recipient, amount })?,
            funds: vec![],
        }
        .into(),
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    }];
    if let Some(generator) = generator {
        messages.push(SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: lp_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: generator.to_string(),
                    amount,
                    expires: None,
                })?,
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
        messages.push(SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: generator.to_string(),
                msg: to_binary(&GeneratorExecuteMsg::DepositFor {
                    lp_token,
                    beneficiary,
                    amount,
                })?,
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }
    Ok(messages)
}
