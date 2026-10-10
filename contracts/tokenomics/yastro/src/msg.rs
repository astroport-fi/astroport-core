use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Coin, Decimal256, Uint128};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllSpenderAllowancesResponse, AllowanceResponse,
    BalanceResponse, Expiration, MarketingInfoResponse, MinterResponse, TokenInfoResponse,
};

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
    /// Astroport Incentives, for forwarding registered pools' rewards to their LP stakers
    pub incentives: Option<String>,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
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
    /// Splits the attached reward tokens across current yASTRO holders, pro-rata. Anyone can
    /// call it; only the reward denoms are accepted.
    DepositRewards {},
    /// Sends the sender's accrued rewards to `recipient` (default: the sender)
    Claim {
        recipient: Option<String>,
    },
    /// Moves a registered pool's accrued rewards into Incentives for that pool's LP stakers,
    /// over the next epoch. Anyone can call it. Funds attached here pay Incentives' fee for a
    /// new reward token on that pool, if it's due.
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

    /// Owner only. Leaves unset fields as they are.
    UpdateConfig {
        add_reward_denoms: Option<Vec<String>>,
        /// Stops accepting these; rewards already deposited stay claimable
        remove_reward_denoms: Option<Vec<String>>,
        /// Applies to unbondings started after the change
        unbonding_period: Option<u64>,
        incentives: Option<String>,
    },
    /// Owner only. Registers a pool holding yASTRO and its LP token, or unregisters it when
    /// `lp_token` is unset.
    SetPool {
        pool: String,
        lp_token: Option<String>,
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
    /// Waiting for yASTRO holders
    pub undistributed: Uint128,
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
    /// Registered pools and their LP tokens
    #[returns(Vec<(Addr, String)>)]
    Pools {},

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
}

#[cw_serde]
pub struct MigrateMsg {}
