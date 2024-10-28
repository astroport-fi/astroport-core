use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Decimal256, Env, StdError, StdResult, Uint128};
use cw20::Cw20ReceiveMsg;

use crate::asset::{Asset, AssetInfo};

/// External incentives schedules must be normalized to 1 week
pub const EPOCH_LENGTH: u64 = 86400 * 7;
/// External incentives schedules aligned to start on Monday. First date: Mon Oct 9 00:00:00 UTC 2023
pub const EPOCHS_START: u64 = 1696809600;
/// Maximum allowed reward schedule duration (~6 month)
pub const MAX_PERIODS: u64 = 25;
/// Maximum allowed external reward tokens per pool
pub const MAX_REWARD_TOKENS: u8 = 5;
/// Validation constraints for max allowed gas limit per one external incentive token transfer.
/// Canonical cw20 transfer gas is typically 130-170k.
/// Native coin bank transfer is 80-90k.
/// Token factory token, for example, xASTRO, with bank hook is ~300k.
/// Setting to 600k seems reasonable for most cases.
/// If token transfer hits this gas limit, reward will be considered as claimed while in reality
/// it will be stuck in the contract.
pub const TOKEN_TRANSFER_GAS_LIMIT: RangeInclusive<u64> = 400_000..=1_500_000u64;

/// Max items per page in queries
pub const MAX_PAGE_LIMIT: u8 = 50;

/// Max number of orphaned rewards to claim at a time
pub const MAX_ORPHANED_REWARD_LIMIT: u8 = 10;

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    pub factory: String,
    pub astro_token: AssetInfo,
    pub vesting_contract: String,
    pub incentivization_fee_info: Option<IncentivizationFeeInfo>,
    pub guardian: Option<String>,
}

#[cw_serde]
pub struct InputSchedule {
    pub reward: Asset,
    pub duration_periods: u64,
}

#[cw_serde]
pub struct IncentivesSchedule {
    /// Schedule start time (matches with epoch start time i.e. on Monday)
    pub next_epoch_start_ts: u64,
    /// Schedule end time (matches with epoch start time i.e. on Monday)
    pub end_ts: u64,
    /// Reward asset info
    pub reward_info: AssetInfo,
    /// Reward per second for the whole schedule
    pub rps: Decimal256,
}

impl IncentivesSchedule {
    /// Creates a new incentives schedule starting now and lasting for the specified number of periods.
    pub fn from_input(env: &Env, input: &InputSchedule) -> StdResult<Self> {
        if input.duration_periods > MAX_PERIODS || input.duration_periods == 0 {
            return Err(StdError::generic_err(format!(
                "Duration must be more 0 and less than or equal to {MAX_PERIODS}",
            )));
        }

        let block_ts = env.block.time.seconds();

        let rem = block_ts % EPOCHS_START;
        // If rem == 0 then we are at the beginning of the current epoch.
        // To keep logic consistent, we always add 1 week more.
        // Hence, minimal possible duration varies from 7 days 1 second to 14 days,
        // which depends on how far from Monday block time is.
        let next_epoch_start_ts = EPOCHS_START + (rem / EPOCH_LENGTH + 1) * EPOCH_LENGTH;
        let end_ts = next_epoch_start_ts + input.duration_periods * EPOCH_LENGTH;

        let rps = Decimal256::from_ratio(input.reward.amount, end_ts - block_ts);

        if rps < Decimal256::one() {
            return Err(StdError::generic_err(format!(
                "Reward per second must be at least 1 unit but actual is {rps}",
            )));
        }

        Ok(Self {
            next_epoch_start_ts,
            end_ts,
            reward_info: input.reward.info.clone(),
            rps,
        })
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Setup generators with their respective allocation points.
    /// Only the owner or generator controller can execute this.
    SetupPools {
        /// The list of pools with allocation point.
        pools: Vec<(String, Uint128)>,
    },
    /// Update rewards and return it to user.
    ClaimRewards {
        /// The LP token cw20 address or token factory denom
        lp_tokens: Vec<String>,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`]. Handles cw20 LP token deposits.
    Receive(Cw20ReceiveMsg),
    /// Stake LP tokens in the Generator. LP tokens staked on behalf of recipient if recipient is set.
    /// Otherwise LP tokens are staked on behalf of message sender.
    Deposit { recipient: Option<String> },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The LP token cw20 address or token factory denom
        lp_token: String,
        /// The amount to withdraw. Must not exceed total staked amount.
        amount: Uint128,
    },
    /// Set a new amount of ASTRO to distribute per seconds.
    /// Only the owner can execute this.
    SetTokensPerSecond {
        /// The new amount of ASTRO to distribute per second
        amount: Uint128,
    },
    /// Incentivize a pool with external rewards. Rewards can be in either native or cw20 form.
    /// Incentivizor must send incentivization fee along with rewards (if this reward token is new in this pool).
    /// 3rd parties are encouraged to keep endless schedules without breaks even with the small rewards.
    /// Otherwise, reward token will be removed from the pool info and go to outstanding rewards.
    /// Next schedules with the same token will be considered as "new".  
    /// NOTE: Sender must approve allowance for cw20 reward tokens to this contract.
    Incentivize {
        /// The LP token cw20 address or token factory denom
        lp_token: String,
        /// Incentives schedule
        schedule: InputSchedule,
    },
    /// Same as Incentivize endpoint but for multiple pools in one go.
    IncentivizeMany(Vec<(String, InputSchedule)>),
    /// Remove specific reward token from the pool.
    /// Only the owner can execute this.
    RemoveRewardFromPool {
        /// The LP token cw20 address or token factory denom
        lp_token: String,
        /// The reward token cw20 address or token factory denom
        reward: String,
        /// If there is too much spam in the state, owner can bypass upcoming schedules;
        /// Tokens from these schedules will stuck in Generator balance forever.
        /// Set true only in emergency cases i.e. if deregistration message hits gas limit during simulation.
        /// Default: false
        #[serde(default)]
        bypass_upcoming_schedules: bool,
        /// Receiver of unclaimed rewards
        receiver: String,
    },
    /// Claim all or up to the limit accumulated orphaned rewards.
    /// Only the owner can execute this.
    ClaimOrphanedRewards {
        /// Number of assets to claim
        limit: Option<u8>,
        /// Receiver of orphaned rewards
        receiver: String,
    },
    /// Update config.
    /// Only the owner can execute it.
    UpdateConfig {
        /// The new ASTRO token info
        astro_token: Option<AssetInfo>,
        /// The new vesting contract address
        vesting_contract: Option<String>,
        /// The new generator controller contract address
        generator_controller: Option<String>,
        /// The new generator guardian
        guardian: Option<String>,
        /// New incentivization fee info
        incentivization_fee_info: Option<IncentivizationFeeInfo>,
        /// New external incentive token transfer gas limit
        token_transfer_gas_limit: Option<u64>,
    },
    /// Add or remove token to the block list.
    /// Only owner or guardian can execute this.
    /// Pools which contain these tokens can't be incentivized with ASTRO rewards.
    /// Also blocked tokens can't be used as external reward.
    /// Current active pools with these tokens will be removed from active set.
    UpdateBlockedTokenslist {
        /// Tokens to add
        #[serde(default)]
        add: Vec<AssetInfo>,
        /// Tokens to remove
        #[serde(default)]
        remove: Vec<AssetInfo>,
    },
    /// Only factory can set the allocation points to zero for the specified pool.
    /// Initiated from deregistration context in factory.
    DeactivatePool { lp_token: String },
    /// Go through active pools and deactivate the ones which pair type is blocked
    DeactivateBlockedPools {},
    /// Creates a request to change contract ownership
    /// Only the current owner can execute this.
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the contract owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
}

#[cw_serde]
/// Cw20 hook message template
pub enum Cw20Msg {
    Deposit {
        recipient: Option<String>,
    },
    /// Besides this enum variant is redundant we keep this for backward compatibility with old pair contracts
    DepositFor(String),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns the main contract parameters
    #[returns(Config)]
    Config {},
    /// Deposit returns the LP token amount deposited in a specific generator
    #[returns(Uint128)]
    Deposit { lp_token: String, user: String },
    /// PendingToken returns the amount of rewards that can be claimed by an account that deposited a specific LP token in a generator
    #[returns(Vec<Asset>)]
    PendingRewards { lp_token: String, user: String },
    /// RewardInfo returns reward information for a specified LP token
    #[returns(Vec<RewardInfo>)]
    RewardInfo { lp_token: String },
    /// PoolInfo returns information about a pool associated with the specified LP token
    #[returns(PoolInfoResponse)]
    PoolInfo { lp_token: String },
    /// Returns a list of tuples with addresses and their staked amount
    #[returns(Vec<(String, Uint128)>)]
    PoolStakers {
        lp_token: String,
        start_after: Option<String>,
        limit: Option<u8>,
    },
    /// Returns paginated list of blocked tokens
    #[returns(Vec<AssetInfo>)]
    BlockedTokensList {
        start_after: Option<AssetInfo>,
        limit: Option<u8>,
    },
    /// Checks whether fee expected for the specified pool if user wants to add new reward schedule
    #[returns(bool)]
    IsFeeExpected { lp_token: String, reward: String },
    /// Returns the list of all external reward schedules for the specified LP token
    #[returns(Vec<ScheduleResponse>)]
    ExternalRewardSchedules {
        /// Reward cw20 addr/denom
        reward: String,
        lp_token: String,
        /// Start after specified timestamp
        start_after: Option<u64>,
        /// Limit number of returned schedules.
        limit: Option<u8>,
    },
    #[returns(Vec<String>)]
    /// Returns the list of all ever incentivized pools
    ListPools {
        /// Start after specified LP token
        start_after: Option<String>,
        /// Limit number of returned pools.
        limit: Option<u8>,
    },
    #[returns(Vec<(String, Uint128)>)]
    /// Returns the list of all pools receiving astro emissions
    ActivePools {},
}

#[cw_serde]
pub struct IncentivizationFeeInfo {
    /// Fee receiver can be either a contract or a wallet.
    pub fee_receiver: Addr,
    /// To make things easier we avoid CW20 fee tokens
    pub fee: Coin,
}

#[cw_serde]
pub struct Config {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// [`AssetInfo`] of the ASTRO token
    pub astro_token: AssetInfo,
    /// Total amount of ASTRO rewards per second
    pub astro_per_second: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_points: Uint128,
    /// The vesting contract which distributes internal (ASTRO) rewards
    pub vesting_contract: Addr,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
    /// Defines native fee along with fee receiver.
    /// Fee is paid on adding NEW external reward to a specific pool
    pub incentivization_fee_info: Option<IncentivizationFeeInfo>,
    /// Max allowed gas limit per one external incentive token transfer.
    /// If token transfer hits this gas limit, reward will be considered as claimed while in reality
    /// it will be stuck in the contract.
    /// If None, there is no gas limit.
    pub token_transfer_gas_limit: Option<u64>,
}

#[cw_serde]
#[derive(Eq)]
/// This enum is a tiny wrapper over [`AssetInfo`] to differentiate between internal and external rewards.
/// External rewards always have a next_update_ts field which is used to update reward per second (or disable them).
pub enum RewardType {
    /// Internal rewards aka ASTRO emissions don't have next_update_ts field and they are paid out from Vesting contract.
    Int(AssetInfo),
    /// External rewards always have corresponding schedules. Reward is paid out from Incentives contract balance.
    Ext {
        info: AssetInfo,
        /// Time when next schedule should start
        next_update_ts: u64,
    },
}

impl RewardType {
    pub fn is_external(&self) -> bool {
        matches!(&self, RewardType::Ext { .. })
    }

    pub fn asset_info(&self) -> &AssetInfo {
        match &self {
            RewardType::Int(info) | RewardType::Ext { info, .. } => info,
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        match (&self, other) {
            (RewardType::Int(..), RewardType::Int(..)) => true,
            (RewardType::Ext { info: info1, .. }, RewardType::Ext { info: info2, .. }) => {
                info1 == info2
            }
            _ => false,
        }
    }
}

impl Hash for RewardType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // We ignore next_update_ts field to have the same hash for the same external reward token
        match self {
            RewardType::Int(info) => {
                state.write_u8(0);
                info.hash(state);
            }
            RewardType::Ext { info, .. } => {
                state.write_u8(1);
                info.hash(state);
            }
        }
    }

    #[cfg(not(tarpaulin_include))]
    fn hash_slice<H: Hasher>(data: &[Self], state: &mut H)
    where
        Self: Sized,
    {
        for d in data {
            d.hash(state);
        }
    }
}

#[cw_serde]
pub struct RewardInfo {
    /// Defines [`AssetInfo`] of reward token as well as its type: protocol or external.
    pub reward: RewardType,
    /// Reward tokens per second for the whole pool
    pub rps: Decimal256,
    /// Last checkpointed reward per LP token
    pub index: Decimal256,
    /// Orphaned rewards might appear between the time when pool
    /// gets incentivized and the time when first user stakes
    pub orphaned: Decimal256,
}

#[cw_serde]
pub struct PoolInfoResponse {
    /// Total amount of LP tokens staked in this pool
    pub total_lp: Uint128,
    /// Vector contains reward info for each reward token
    pub rewards: Vec<RewardInfo>,
    /// Last time when reward indexes were updated
    pub last_update_ts: u64,
}

#[cw_serde]
pub struct ScheduleResponse {
    pub rps: Decimal256,
    pub start_ts: u64,
    pub end_ts: u64,
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Timestamp;

    use crate::asset::AssetInfoExt;

    use super::*;

    #[test]
    fn test_schedules() {
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(EPOCHS_START);

        let schedule = IncentivesSchedule::from_input(
            &env,
            &InputSchedule {
                reward: AssetInfo::native("test").with_balance(2 * EPOCH_LENGTH),
                duration_periods: 1,
            },
        )
        .unwrap();

        assert_eq!(schedule.next_epoch_start_ts, EPOCHS_START + EPOCH_LENGTH);
        assert_eq!(schedule.end_ts, schedule.next_epoch_start_ts + EPOCH_LENGTH);
        assert_eq!(schedule.rps, Decimal256::one());

        let err = IncentivesSchedule::from_input(
            &env,
            &InputSchedule {
                reward: AssetInfo::native("test").with_balance(100000000u128),
                duration_periods: 0,
            },
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "Generic error: Duration must be more 0 and less than or equal to {MAX_PERIODS}"
            )
        );

        let err = IncentivesSchedule::from_input(
            &env,
            &InputSchedule {
                reward: AssetInfo::native("test").with_balance(100000000u128),
                duration_periods: MAX_PERIODS + 1,
            },
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "Generic error: Duration must be more 0 and less than or equal to {MAX_PERIODS}"
            )
        );

        let err = IncentivesSchedule::from_input(
            &env,
            &InputSchedule {
                reward: AssetInfo::native("test").with_balance(100000u128),
                duration_periods: MAX_PERIODS,
            },
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .starts_with("Generic error: Reward per second must be at least 1 unit"),
            "Unexpected error: {}",
            err.to_string()
        );

        env.block.time = Timestamp::from_seconds(EPOCHS_START + 10 * EPOCH_LENGTH + 3 * 86400);
        let schedule = IncentivesSchedule::from_input(
            &env,
            &InputSchedule {
                // 4 days from current week + 21 days more
                reward: AssetInfo::native("test").with_balance(25 * 86400u64),
                duration_periods: 3,
            },
        )
        .unwrap();

        assert_eq!(schedule.next_epoch_start_ts, 1703462400);
        assert_eq!(
            schedule.end_ts,
            schedule.next_epoch_start_ts + 3 * EPOCH_LENGTH
        );
        assert_eq!(schedule.rps, Decimal256::one());
    }
}
