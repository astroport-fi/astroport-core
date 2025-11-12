use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

use crate::asset::AssetInfo;

/// This enum describes a swap operation.
#[cw_serde]
pub struct SwapOperation {
    /// The address of the pair contract
    pub pair_address: String,
    /// Information about the asset being swapped
    pub offer_asset_info: AssetInfo,
    /// Information about the asset we swap to
    pub ask_asset_info: AssetInfo,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
    /// ExecuteSwapOperations processes multiple swaps while mentioning the minimum amount of tokens to receive for the last swap operation
    ExecuteSwapOperations {
        operations: Vec<SwapOperation>,
        minimum_receive: Option<Uint128>,
        to: Option<String>,
        max_spread: Option<Decimal>,
    },

    /// Internal use
    /// ExecuteSwapOperation executes a single swap operation
    ExecuteSwapOperation {
        operation: SwapOperation,
        to: Option<String>,
        max_spread: Option<Decimal>,
        single: bool,
    },
}

#[cw_serde]
pub struct SwapResponseData {
    pub return_amount: Uint128,
}

#[cw_serde]
pub enum Cw20HookMsg {
    ExecuteSwapOperations {
        /// A vector of swap operations
        operations: Vec<SwapOperation>,
        /// The minimum amount of tokens to get from a swap
        minimum_receive: Option<Uint128>,
        /// The recipient
        to: Option<String>,
        /// Max spread
        max_spread: Option<Decimal>,
    },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// SimulateSwapOperations simulates multi-hop swap operations
    #[returns(Uint128)]
    SimulateSwapOperations {
        /// The amount of tokens to swap
        offer_amount: Uint128,
        /// The swap operations to perform, each swap involving a specific pool
        operations: Vec<SwapOperation>,
    },
    #[returns(Uint128)]
    ReverseSimulateSwapOperations {
        /// The amount of tokens one is willing to receive
        ask_amount: Uint128,
        /// The swap operations to perform, each swap involving a specific pool
        operations: Vec<SwapOperation>,
    },
}
