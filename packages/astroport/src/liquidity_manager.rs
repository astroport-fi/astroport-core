use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;

use crate::asset::{Asset, AssetInfo, PairInfo};
use crate::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, FeeShareConfig};

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

/// Stable swap config which is used in raw queries. It's compatible with v1, v2 and v3 stable pair contract.
#[cw_serde]
pub struct CompatPairStableConfig {
    /// The contract owner
    pub owner: Option<Addr>,
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract update the asset cumulative prices
    pub block_time_last: u64,
    /// This is the current amplification used in the pool
    pub init_amp: u64,
    /// This is the start time when amplification starts to scale up or down
    pub init_amp_time: u64,
    /// This is the target amplification to reach at `next_amp_time`
    pub next_amp: u64,
    /// This is the timestamp when the current pool amplification should be `next_amp`
    pub next_amp_time: u64,

    // Fields below are added for compatability with v1 and v2
    /// The greatest precision of assets in the pool
    pub greatest_precision: Option<u8>,
    /// The vector contains cumulative prices for each pair of assets in the pool
    #[serde(default)]
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Option<Uint128>,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Option<Uint128>,
    // Fee sharing configuration
    pub fee_share: Option<FeeShareConfig>,
}
