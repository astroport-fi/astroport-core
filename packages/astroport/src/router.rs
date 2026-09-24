use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

use crate::asset::AssetInfo;

pub const MAX_SWAP_OPERATIONS: usize = 50;

/// This structure holds the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The astroport factory contract address
    pub astroport_factory: String,
}

/// This enum describes a swap operation.
#[cw_serde]
pub enum SwapOperation {
    /// Swap in the factory's pool for this asset pair
    AstroSwap {
        /// Information about the asset being swapped
        offer_asset_info: AssetInfo,
        /// Information about the asset we swap to
        ask_asset_info: AssetInfo,
    },
    /// Swap in a specific pool, given by address. Unlike [`SwapOperation::AstroSwap`] the pool
    /// doesn't have to be registered in the factory, so standalone pools (e.g. a transmuter
    /// instantiated directly) can be part of a multi-hop route. The pool must report both assets
    /// in its `pair {}` query.
    PoolSwap {
        /// The pool contract address
        pool_addr: String,
        /// Information about the asset being swapped
        offer_asset_info: AssetInfo,
        /// Information about the asset we swap to
        ask_asset_info: AssetInfo,
    },
}

impl SwapOperation {
    pub fn get_target_asset_info(&self) -> AssetInfo {
        match self {
            SwapOperation::AstroSwap { ask_asset_info, .. }
            | SwapOperation::PoolSwap { ask_asset_info, .. } => ask_asset_info.clone(),
        }
    }
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
    /// ExecuteSwapOperations swaps the sent amount along the route. Pools' own spread checks are
    /// disabled on every hop; `minimum_receive` on the final output is the swap's only price
    /// guarantee, so it's required and must be greater than zero.
    ExecuteSwapOperations {
        operations: Vec<SwapOperation>,
        minimum_receive: Uint128,
        to: Option<String>,
    },

    /// Internal use
    /// ExecuteSwapOperation executes a single swap operation
    ExecuteSwapOperation {
        operation: SwapOperation,
        to: Option<String>,
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
        /// The minimum amount of tokens to get from the swap; required and greater than zero
        minimum_receive: Uint128,
        /// The recipient
        to: Option<String>,
    },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns configuration parameters for the contract using a custom [`ConfigResponse`] structure
    #[returns(ConfigResponse)]
    Config {},
    /// SimulateSwapOperations simulates multi-hop swap operations
    #[returns(SimulateSwapOperationsResponse)]
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

/// This structure describes a custom struct to return a query response containing the base contract configuration.
#[cw_serde]
pub struct ConfigResponse {
    /// The Astroport factory contract address
    pub astroport_factory: String,
}

/// This structure describes a custom struct to return a query response containing the end amount of a swap simulation
#[cw_serde]
pub struct SimulateSwapOperationsResponse {
    /// The amount of tokens received in a swap simulation
    pub amount: Uint128,
}
