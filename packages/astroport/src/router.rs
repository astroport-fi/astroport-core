use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;

use crate::asset::AssetInfo;

pub const MAX_SWAP_OPERATIONS: usize = 50;

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the astroport factory contract address
    pub astroport_factory: String,
}

/// ## Description
/// This enum describes the swap operation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwapOperation {
    /// Native swap
    NativeSwap {
        /// the offer denom
        offer_denom: String,
        /// the asks denom
        ask_denom: String,
    },
    /// ASTRO swap
    AstroSwap {
        /// the offer asset info
        offer_asset_info: AssetInfo,
        /// the asks asset info
        ask_asset_info: AssetInfo,
    },
}

impl SwapOperation {
    pub fn get_target_asset_info(&self) -> AssetInfo {
        match self {
            SwapOperation::NativeSwap { ask_denom, .. } => AssetInfo::NativeToken {
                denom: ask_denom.clone(),
            },
            SwapOperation::AstroSwap { ask_asset_info, .. } => ask_asset_info.clone(),
        }
    }
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received
    /// template.
    Receive(Cw20ReceiveMsg),
    /// Execute multiple BuyOperation
    ExecuteSwapOperations {
        operations: Vec<SwapOperation>,
        minimum_receive: Option<Uint128>,
        to: Option<Addr>,
    },

    /// Internal use
    /// Swap all offer tokens to ask token
    ExecuteSwapOperation {
        operation: SwapOperation,
        to: Option<String>,
    },
    /// Internal use
    /// Check the swap amount is exceed minimum_receive
    AssertMinimumReceive {
        asset_info: AssetInfo,
        prev_balance: Uint128,
        minimum_receive: Uint128,
        receiver: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    ExecuteSwapOperations {
        /// operations for swap
        operations: Vec<SwapOperation>,
        /// the minimum receive for swap
        minimum_receive: Option<Uint128>,
        /// the recipient
        to: Option<String>,
    },
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns controls settings that specified in custom [`ConfigResponse`] structure
    Config {},
    /// Simulates multi-hop swap operations
    SimulateSwapOperations {
        /// the offer amount
        offer_amount: Uint128,
        /// operations for swap
        operations: Vec<SwapOperation>,
    },
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// the astroport factory contract address
    pub astroport_factory: String,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulateSwapOperationsResponse {
    /// the amount of swap
    pub amount: Uint128,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
