use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// T = String (unchecked) or Addr (checked)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config<T> {
    /// Account who can create new allocations
    pub owner: T,
    /// Account to receive the refund of unvested tokens if a user terminates allocation
    pub refund_recipient: T,
    /// Address of ASTRO token
    pub astro_token: T,
    /// By default, unlocking starts at Astroport launch, with a cliff of 6 months and a duration of 36 months.
    /// If not specified, all allocations use this default schedule
    pub default_unlock_schedule: Schedule,
}

// Parameters describing a typical vesting/unlocking schedule
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Schedule {
    /// Timestamp of when vesting/unlocking is to be started (in seconds)
    pub start_time: u64,
    /// Number of seconds starting UST during which no token will be vested/unlocked
    pub cliff: u64,
    /// Number of seconds taken since UST for tokens to be fully vested/unlocked
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationParams {
    /// Total amount of ASTRO token allocated to this account
    pub amount: Uint128,
    /// Parameters controlling the vesting process
    pub vest_schedule: Schedule,
    /// Parameters controlling the unlocking process
    /// If not provided, use `config.default_unlock_schedule`
    pub unlock_schedule: Option<Schedule>,
    /// proposed new_receiver who will get the allocation
    pub proposed_receiver: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationStatus {
    /// Amount of ASTRO already withdrawn
    pub astro_withdrawn: Uint128,
}

impl AllocationStatus {
    pub const fn new() -> Self {
        Self {
            astro_withdrawn: Uint128::zero(),
        }
    }
}

pub mod msg {
    use cosmwasm_std::{Addr, Uint128};
    use cw20::Cw20ReceiveMsg;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::{AllocationParams, AllocationStatus, Config};

    pub type InstantiateMsg = Config<String>;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecuteMsg {
        /// Implementation of cw20 receive msg
        Receive(Cw20ReceiveMsg),
        /// Claim withdrawable ASTRO
        Withdraw {},
        /// Give up allocation, refund all unvested tokens to `config.fallback_recipient`
        Terminate {},
        /// Update addresses of owner and fallback_recipient
        TransferOwnership {
            new_owner: String,
            new_refund_recipient: String,
        },
        /// Allows users to change the receiver address of their allocations etc
        ProposeNewReceiver { new_receiver: String },
        /// Allows users to remove the previously proposed new receiver for their allocations
        DropNewReceiver {},
        /// Allows new receivers to claim the allocations
        ClaimReceiver { prev_receiver: String },
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ReceiveMsg {
        /// Create new allocations
        CreateAllocations {
            allocations: Vec<(String, AllocationParams)>,
        },
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum QueryMsg {
        // Config of this contract
        Config {},
        // Parameters and current status of an allocation
        Allocation { account: String },
        // Simulate how many ASTRO will be released if a withdrawal is attempted
        SimulateWithdraw { account: String },
        // Total amount of xASTRO owned by an account that's under custody by this contract
        // Used by Martian Council to determine the account's vested voting power
        // VotingPowerAt { account: String, block: u64 },
    }

    pub type ConfigResponse = Config<Addr>;
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct AllocationResponse {
        pub params: AllocationParams,
        pub status: AllocationStatus,
        // pub voting_power_snapshots: Vec<(u64, Uint128)>,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct SimulateWithdrawResponse {
        /// Amount of ASTRO to receive
        pub astro_to_withdraw: Uint128,
    }
}
