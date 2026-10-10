use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Coin, Decimal256, Uint128};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllSpenderAllowancesResponse, AllowanceResponse,
    BalanceResponse, DownloadLogoResponse, Expiration, Logo, MarketingInfoResponse, MinterResponse,
    TokenInfoResponse,
};

use cw20_base::msg::InstantiateMarketingInfo;

use crate::state::{Config, Unbonding};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    /// The denom staked for yASTRO, 1:1
    pub astro_denom: String,
    /// Seconds between unstaking and being able to withdraw
    pub unbonding_period: u64,
    /// Denoms `deposit_rewards` accepts
    pub reward_denoms: Vec<String>,
    /// Astroport Incentives, for forwarding pools' rewards to their LP stakers
    pub incentives: Option<String>,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    /// Project, description, logo, and the address that can update them
    pub marketing: Option<InstantiateMarketingInfo>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Stakes the attached ASTRO for the same amount of yASTRO, minted to `recipient` (default:
    /// the sender)
    Stake {
        recipient: Option<String>,
    },
    /// Burns `amount` of the sender's yASTRO and starts its unbonding. Unbonding ASTRO earns
    /// nothing.
    Unstake {
        amount: Uint128,
    },
    /// Sends the sender every unbonding that has matured
    Withdraw {},
    /// Queues the attached reward tokens to stream to yASTRO holders over the next epoch
    /// (weeks starting Monday 00:00 UTC), pro-rata to balance and time held. Anyone can call it;
    /// only the reward denoms are accepted.
    DepositRewards {},
    /// Sends the sender's accrued rewards to `recipient` (default: the sender)
    Claim {
        recipient: Option<String>,
    },
    /// Sends `address`'s accrued rewards to `address` itself. Anyone can call it, for holders
    /// that can't call `claim` (DAO treasuries, vaults). Pools use `forward_pool_rewards`.
    ClaimFor {
        address: String,
    },
    /// Moves a pool's accrued rewards into Incentives for that pool's LP stakers, as a schedule
    /// lasting until the next Monday plus one week. Anyone can call it, for any contract that
    /// answers Astroport's `pair {}` query with yASTRO among its assets. Needs LP staked in
    /// Incentives, and yASTRO exempt from Incentives' fee. A reward Incentives rejects, or one
    /// too small to schedule yet, stays with the pool for the next call.
    ForwardPoolRewards {
        pool: String,
    },

    // CW20, with every balance change settling both sides' rewards first
    Transfer {
        recipient: String,
        amount: Uint128,
    },
    Send {
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    SendFrom {
        owner: String,
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    /// Only the marketing address
    UpdateMarketing {
        project: Option<String>,
        description: Option<String>,
        marketing: Option<String>,
    },
    /// Only the marketing address
    UploadLogo(Logo),

    /// Owner only. Leaves unset fields as they are.
    UpdateConfig {
        add_reward_denoms: Option<Vec<String>>,
        /// Stops accepting these; rewards already deposited stay claimable
        remove_reward_denoms: Option<Vec<String>>,
        /// Applies to unbondings started after the change. At most 28 days.
        unbonding_period: Option<u64>,
        incentives: Option<String>,
    },
    ProposeNewOwner {
        owner: String,
        expires_in: u64,
    },
    DropOwnershipProposal {},
    ClaimOwnership {},
}

#[cw_serde]
pub struct RewardState {
    pub denom: String,
    /// Reward per yASTRO paid out so far
    pub index: Decimal256,
    /// Paid out per second until `epoch_end`, shared by all yASTRO. APR is
    /// `rate * 31_536_000 / total_staked`, in reward per ASTRO.
    pub rate: Decimal256,
    /// When the current epoch ends
    pub epoch_end: u64,
    /// Streams over the next epoch
    pub queued: Decimal256,
    /// Everything ever deposited
    pub total_deposited: Uint128,
    /// Whether `deposit_rewards` still accepts it
    pub accepted: bool,
}

#[cw_serde]
pub struct StakingState {
    /// yASTRO supply, which is the ASTRO staked
    pub total_staked: Uint128,
    pub total_unbonding: Uint128,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    /// Rewards an address can claim right now
    #[returns(Vec<Coin>)]
    PendingRewards { address: String },
    #[returns(Vec<Unbonding>)]
    Unbondings { address: String },
    #[returns(Vec<RewardState>)]
    RewardState {},
    #[returns(StakingState)]
    StakingState {},

    // CW20
    #[returns(BalanceResponse)]
    Balance { address: String },
    #[returns(TokenInfoResponse)]
    TokenInfo {},
    /// Always empty: yASTRO is only minted by staking
    #[returns(Option<MinterResponse>)]
    Minter {},
    #[returns(AllowanceResponse)]
    Allowance { owner: String, spender: String },
    #[returns(AllAllowancesResponse)]
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(AllSpenderAllowancesResponse)]
    AllSpenderAllowances {
        spender: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(AllAccountsResponse)]
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(MarketingInfoResponse)]
    MarketingInfo {},
    #[returns(DownloadLogoResponse)]
    DownloadLogo {},
}

#[cw_serde]
pub struct MigrateMsg {}
