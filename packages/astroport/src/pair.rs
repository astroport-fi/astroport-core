use crate::observation::OracleObservation;
use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::asset::{Asset, AssetInfo, PairInfo};

use cosmwasm_std::{Addr, Binary, Decimal, Decimal256, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;

/// The default swap slippage
pub const DEFAULT_SLIPPAGE: &str = "0.005";
/// The maximum allowed swap slippage
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";
/// The maximum fee share allowed, 10%
pub const MAX_FEE_SHARE_BPS: u16 = 1000;

/// Decimal precision for TWAP results
pub const TWAP_PRECISION: u8 = 6;

/// Min safe trading size (0.00001) to calculate a price. This value considers
/// amount in decimal form with respective token precision.
pub const MIN_TRADE_SIZE: Decimal256 = Decimal256::raw(10000000000000);

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Information about assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// The token contract code ID used for the tokens in the pool
    pub token_code_id: u64,
    /// The factory contract address
    pub factory_addr: String,
    /// Optional binary serialised parameters for custom pool types
    pub init_params: Option<Binary>,
}

/// This structure describes the execute messages available in the contract.
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
}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// Swap a given amount of asset
    Swap {
        ask_asset_info: Option<AssetInfo>,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Withdraw liquidity from the pool
    WithdrawLiquidity {
        #[serde(default)]
        assets: Vec<Asset>,
    },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`super::asset::PairInfo`].
    #[returns(PairInfo)]
    Pair {},
    /// Returns information about a pool in an object of type [`PoolResponse`].
    #[returns(PoolResponse)]
    Pool {},
    /// Returns contract configuration settings in a custom [`ConfigResponse`] structure.
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about the share of the pool in a vector that contains objects of type [`Asset`].
    #[returns(Vec<Asset>)]
    Share { amount: Uint128 },
    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    #[returns(SimulationResponse)]
    Simulation {
        offer_asset: Asset,
        ask_asset_info: Option<AssetInfo>,
    },
    /// Returns information about cumulative prices in a [`ReverseSimulationResponse`] object.
    #[returns(ReverseSimulationResponse)]
    ReverseSimulation {
        offer_asset_info: Option<AssetInfo>,
        ask_asset: Asset,
    },
    /// Returns information about the cumulative prices in a [`CumulativePricesResponse`] object
    #[returns(CumulativePricesResponse)]
    CumulativePrices {},
    /// Returns current D invariant in as a [`u128`] value
    #[returns(Uint128)]
    QueryComputeD {},
    /// Returns the balance of the specified asset that was in the pool just preceeding the moment of the specified block height creation.
    #[returns(Option<Uint128>)]
    AssetBalanceAt {
        asset_info: AssetInfo,
        block_height: Uint64,
    },
    /// Query price from observations
    #[returns(OracleObservation)]
    Observe { seconds_ago: u64 },
}

/// This struct is used to return a query result with the total amount of LP tokens and assets in a specific pool.
#[cw_serde]
pub struct PoolResponse {
    /// The assets in the pool together with asset amounts
    pub assets: Vec<Asset>,
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
}

/// This struct is used to return a query result with the general contract configuration.
#[cw_serde]
pub struct ConfigResponse {
    /// Last timestamp when the cumulative prices in the pool were updated
    pub block_time_last: u64,
    /// The pool's parameters
    pub params: Option<Binary>,
    /// The contract owner
    pub owner: Addr,
    /// The factory contract address
    pub factory_addr: Addr,
}

/// Holds the configuration for fee sharing
#[cw_serde]
pub struct FeeShareConfig {
    /// The fee shared with the address
    pub bps: u16,
    /// The share is sent to this address on every swap
    pub recipient: Addr,
}

/// This structure holds the parameters that are returned from a swap simulation response
#[cw_serde]
pub struct SimulationResponse {
    /// The amount of ask assets returned by the swap
    pub return_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure holds the parameters that are returned from a reverse swap simulation response.
#[cw_serde]
pub struct ReverseSimulationResponse {
    /// The amount of offer assets returned by the reverse swap
    pub offer_amount: Uint128,
    /// The spread used in the swap operation
    pub spread_amount: Uint128,
    /// The amount of fees charged by the transaction
    pub commission_amount: Uint128,
}

/// This structure is used to return a cumulative prices query response.
#[cw_serde]
pub struct CumulativePricesResponse {
    /// The assets in the pool to query
    pub assets: Vec<Asset>,
    /// The total amount of LP tokens currently issued
    pub total_share: Uint128,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// This structure holds XYK pool parameters.
#[cw_serde]
pub struct XYKPoolParams {
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: Option<bool>,
}

/// This structure stores a XYK pool's configuration.
#[cw_serde]
pub struct XYKPoolConfig {
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
    // The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
}

/// This enum stores the option available to enable asset balances tracking over blocks.
#[cw_serde]
pub enum XYKPoolUpdateParams {
    /// Enables asset balances tracking over blocks.
    EnableAssetBalancesTracking,
    /// Enables the sharing of swap fees with an external party.
    EnableFeeShare {
        /// The fee shared with the fee_share_address
        fee_share_bps: u16,
        /// The fee_share_bps is sent to this address on every swap
        fee_share_address: String,
    },
    DisableFeeShare,
}

/// This structure holds stableswap pool parameters.
#[cw_serde]
pub struct StablePoolParams {
    /// The current stableswap pool amplification
    pub amp: u64,
    /// The contract owner
    pub owner: Option<String>,
}

/// This structure stores a stableswap pool's configuration.
#[cw_serde]
pub struct StablePoolConfig {
    /// The stableswap pool amplification
    pub amp: Decimal,
    // The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
}

/// This enum stores the options available to start and stop changing a stableswap pool's amplification.
#[cw_serde]
pub enum StablePoolUpdateParams {
    StartChangingAmp {
        next_amp: u64,
        next_amp_time: u64,
    },
    StopChangingAmp {},
    /// Enables the sharing of swap fees with an external party.
    EnableFeeShare {
        /// The fee shared with the fee_share_address
        fee_share_bps: u16,
        /// The fee_share_bps is sent to this address on every swap
        fee_share_address: String,
    },
    DisableFeeShare,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::native_asset_info;
    use cosmwasm_std::{from_json, to_json_binary};

    #[cw_serde]
    pub struct LegacyInstantiateMsg {
        pub asset_infos: [AssetInfo; 2],
        pub token_code_id: u64,
        pub factory_addr: String,
        pub init_params: Option<Binary>,
    }

    #[cw_serde]
    pub struct LegacyConfigResponse {
        pub block_time_last: u64,
        pub params: Option<Binary>,
        pub factory_addr: Addr,
        pub owner: Addr,
    }

    #[test]
    fn test_init_msg_compatability() {
        let inst_msg = LegacyInstantiateMsg {
            asset_infos: [
                native_asset_info("uusd".to_string()),
                native_asset_info("uluna".to_string()),
            ],
            token_code_id: 0,
            factory_addr: "factory".to_string(),
            init_params: None,
        };

        let ser_msg = to_json_binary(&inst_msg).unwrap();
        // This .unwrap() is enough to make sure that [AssetInfo; 2] and Vec<AssetInfo> are compatible.
        let _: InstantiateMsg = from_json(&ser_msg).unwrap();
    }

    #[test]
    fn test_config_response_compatability() {
        let ser_msg = to_json_binary(&LegacyConfigResponse {
            block_time_last: 12,
            params: Some(
                to_json_binary(&StablePoolConfig {
                    amp: Decimal::one(),
                    fee_share: None,
                })
                .unwrap(),
            ),
            factory_addr: Addr::unchecked(""),
            owner: Addr::unchecked(""),
        })
        .unwrap();

        let _: ConfigResponse = from_json(&ser_msg).unwrap();
    }

    #[test]
    fn check_empty_vec_deserialization() {
        let variant: Cw20HookMsg = from_json(br#"{"withdraw_liquidity": {} }"#).unwrap();
        assert_eq!(variant, Cw20HookMsg::WithdrawLiquidity { assets: vec![] });
    }
}
