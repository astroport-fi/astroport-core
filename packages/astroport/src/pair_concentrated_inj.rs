use crate::observation::OracleObservation;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Decimal, Decimal256, Uint128};
use cw20::Cw20ReceiveMsg;

use crate::asset::PairInfo;
use crate::asset::{Asset, AssetInfo};

use crate::pair::{
    ConfigResponse, CumulativePricesResponse, PoolResponse, ReverseSimulationResponse,
    SimulationResponse,
};
use crate::pair_concentrated::{ConcentratedPoolParams, PromoteParams, UpdatePoolParams};

#[cw_serde]
pub struct OrderbookConfig {
    pub market_id: String,
    pub orders_number: u8,
    pub min_trades_to_avg: u32,
}

/// This structure holds concentrated pool parameters along with orderbook params specific for Injective.
#[cw_serde]
pub struct ConcentratedInjObParams {
    pub main_params: ConcentratedPoolParams,
    pub orderbook_config: OrderbookConfig,
}

/// This structure is extended version of [`crate::pair::ExecuteMsg`].
#[cw_serde]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ProvideLiquidity allows someone to provide liquidity in the pool
    ProvideLiquidity {
        /// The assets available in the pool
        assets: Vec<Asset>,
        /// The slippage tolerance that allows liquidity provision only if the price in the pool doesn't move too much
        slippage_tolerance: Option<Decimal>,
        /// Determines whether the LP tokens minted for the user is auto_staked in the Generator contract
        auto_stake: Option<bool>,
        /// The receiver of LP tokens
        receiver: Option<String>,
    },
    /// Swap performs a swap in the pool
    Swap {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Update the pair configuration
    UpdateConfig { params: Binary },
    /// ProposeNewOwner creates a proposal to change contract ownership.
    /// The validity period for the proposal is set in the `expires_in` variable.
    ProposeNewOwner {
        /// Newly proposed contract owner
        owner: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer to change contract ownership.
    DropOwnershipProposal {},
    /// Used to claim contract ownership.
    ClaimOwnership {},
    /// Permissionless endpoint to withdraw all liquidity from orderbook
    /// if orderbook integration is disabled.
    WithdrawFromOrderbook {},
    /// Permissionless endpoint to update price_tick_size and quantity_tick_size
    /// according to the current exchange module state.
    UpdateMarketTicks {},
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns information about a pair
    #[returns(PairInfo)]
    Pair {},
    /// Returns information about a pool
    #[returns(PoolResponse)]
    Pool {},
    /// Returns contract configuration
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    #[returns(Vec<Asset>)]
    Share { amount: Uint128 },
    /// Returns information about a swap simulation
    #[returns(SimulationResponse)]
    Simulation {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
    },
    /// Returns information about a reverse swap simulation
    #[returns(ReverseSimulationResponse)]
    ReverseSimulation {
        offer_asset_info: Option<AssetInfo>,
        ask_asset: Asset,
    },
    /// Returns information about the cumulative prices
    #[returns(CumulativePricesResponse)]
    CumulativePrices {},
    /// Returns current D invariant
    #[returns(Decimal256)]
    ComputeD {},
    /// Query LP token virtual price
    #[returns(Decimal256)]
    LpPrice {},
    /// Query price from observations
    #[returns(OracleObservation)]
    Observe { seconds_ago: u64 },
    #[returns(OrderbookStateResponse)]
    OrderbookState {},
}

#[cw_serde]
pub struct OrderbookStateResponse {
    /// Market which is being used to deploy liquidity to
    pub market_id: String,
    /// Subaccount used for the orderbook
    pub subaccount: String,
    /// Minimum allowed price tick size in the orderbook
    pub min_price_tick_size: Decimal256,
    /// Minimum allowed quantity tick size in the orderbook
    pub min_quantity_tick_size: Decimal256,
    /// This flag is set when trades, deposits or withdrawals have occurred in the previous block.
    pub need_reconcile: bool,
    /// Last balances of the subaccount on the previous begin blocker
    pub last_balances: Vec<Asset>,
    /// Order number on each side of the orderbook
    pub orders_number: u8,
    /// Minimum number of trades to accumulate average trade size.
    /// Orderbook integration will not be enabled until this number is reached.
    pub min_trades_to_avg: u32,
    /// Whether the pool is ready to integrate with the orderbook (MIN_TRADES_TO_AVG is reached)
    pub ready: bool,
    /// Whether the begin blocker execution is allowed or not. Default: true
    pub enabled: bool,
}

#[cw_serde]
pub enum MigrateMsg {
    MigrateToOrderbook { params: OrderbookConfig },
    Migrate {},
}

/// This enum is intended for parameters update.
#[cw_serde]
pub enum ConcentratedObPoolUpdateParams {
    /// Allows to update fee parameters as well as repeg_profit_threshold, min_price_scale_delta and EMA interval.
    Update(UpdatePoolParams),
    /// Starts gradual (de/in)crease of Amp or Gamma parameters. Can handle an update of both of them.
    Promote(PromoteParams),
    /// Stops Amp and Gamma update and stores current values.
    StopChangingAmpGamma {},
    /// Update orderbook params.
    UpdateOrderbookParams { orders_number: u8 },
}
