use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetInfo, PairInfo};

use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// The maximum allowed swap slippage
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";

/// This structure describes the parameters used for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Information about the two assets in the pool
    pub asset_infos: [AssetInfo; 2],
    /// The token contract code ID used for the tokens in the pool
    pub token_code_id: u64,
    /// The factory contract address
    pub factory_addr: String,
    /// Basic pool parameters
    pub pool_params: PoolParams,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity allows someone to provide liquidity in the pool
    ProvideLiquidity {
        /// The assets available in the pool
        assets: [Asset; 2],
        /// The slippage tolerance that allows liquidity provision only if the price in the pool doesn't move too much
        slippage_tolerance: Option<Decimal>,
        /// The receiver of LP tokens
        receiver: Option<String>,
    },
    /// Swap performs a swap in the pool
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Update whitelist of allowed liquidity providers
    UpdateWhitelist {
        append_addrs: Option<Vec<String>>,
        remove_addrs: Option<Vec<String>>,
    },
    /// Update basic pool parameters
    UpdatePoolParameters { pool_params: PoolParams },
    /// Propose a new owner for the contract
    ProposeNewOwner { new_owner: String, expires_in: u64 },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
}

/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Swap a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Withdraw liquidity from the pool
    WithdrawLiquidity {},
}

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`super::asset::PairInfo`].
    Pair {},
    /// Returns information about a pool in an object of type [`PoolResponse`].
    Pool {},
    /// Returns contract configuration settings in a custom [`ConfigResponse`] structure.
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    Share { amount: Uint128 },
    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    Simulation { offer_asset: Asset },
    /// Returns information about cumulative prices in a [`CumulativePricesResponse`] object.
    ReverseSimulation { ask_asset: Asset },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    CumulativePrices {},
}

/// This struct is used to return a query result with the total amount of LP tokens and the two assets in a specific pool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    /// The assets in the pool together with asset amounts
    pub assets: [Asset; 2],
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct FlowParams {
    pub base_pool: Uint128,
    /// min spread in form of BPS
    pub min_spread: u16,
    /// the number of blocks during which the pool should replenish to equilibrium
    pub recovery_period: u64,
    /// the last block when replenishment was performed
    pub last_repl_block: u64,
    pub pool_delta: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolParams {
    /// UST -> BTC
    pub entry: FlowParams,
    /// BTC -> UST
    pub exit: FlowParams,
    /// BTC/USD
    pub oracles: Vec<Addr>,
}

/// This struct is used to return a query result with the general contract configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// General pair information (e.g pair type)
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The contract owner address
    pub owner: Addr,
    /// Stores list of users who are eligible to provide liquidity
    pub providers_whitelist: Vec<Addr>,
    /// Pool parameters
    pub pool_params: PoolParams,
}

/// This structure holds the parameters that are returned from a swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulationResponse {
    /// The amount of ask assets returned by the swap
    pub return_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure holds the parameters that are returned from a reverse swap simulation response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReverseSimulationResponse {
    /// The amount of offer assets returned by the reverse swap
    pub offer_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure is used to return a cumulative prices query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CumulativePricesResponse {
    /// The two assets in the pool to query
    pub assets: [Asset; 2],
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
    /// The last value for the token0 cumulative price
    pub price0_cumulative_last: Uint128,
    /// The last value for the token1 cumulative price
    pub price1_cumulative_last: Uint128,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
