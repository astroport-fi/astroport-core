use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Uint128};

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The ASTRO token contract address
    pub deposit_token_denom: String,
    /// Tracking contract admin
    pub tracking_admin: String,
    /// The Code ID of contract used to track the TokenFactory token balances
    pub tracking_code_id: u64,
    /// Token factory module address. Contract creator must ensure that the address is exact token factory module address.
    pub token_factory_addr: String,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Deposits ASTRO in exchange for xASTRO
    /// The receiver is optional. If not set, the sender will receive the xASTRO.
    Enter { receiver: Option<String> },
    /// Deposits ASTRO in exchange for xASTRO
    /// and passes **all resulting xASTRO** to defined contract along with an executable message.
    EnterWithHook {
        contract_address: String,
        msg: Binary,
    },
    /// Burns xASTRO in exchange for ASTRO.
    /// The receiver is optional. If not set, the sender will receive the ASTRO.
    Leave { receiver: Option<String> },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns the contract configuration specified in a custom [`Config`] structure
    #[returns(Config)]
    Config {},
    /// Returns xASTRO total supply. Duplicates TotalSupplyAt { timestamp: None } logic but kept for backward compatibility.
    #[returns(Uint128)]
    TotalShares {},
    /// Returns total ASTRO staked in the contract
    #[returns(Uint128)]
    TotalDeposit {},
    #[returns(TrackerData)]
    TrackerConfig {},
    /// BalanceAt returns xASTRO balance of the given address at at the given timestamp.
    /// Returns current balance if timestamp unset.
    #[returns(Uint128)]
    BalanceAt {
        address: String,
        timestamp: Option<u64>,
    },
    /// TotalSupplyAt returns xASTRO total token supply at the given timestamp.
    /// Returns current total supply if timestamp unset.
    #[returns(Uint128)]
    TotalSupplyAt { timestamp: Option<u64> },
}

/// This structure stores the main parameters for the staking contract.
#[cw_serde]
pub struct Config {
    /// The ASTRO token denom
    pub astro_denom: String,
    /// The xASTRO token denom
    pub xastro_denom: String,
}

/// This structure stores the tracking contract data.
#[cw_serde]
pub struct TrackerData {
    /// Tracking contract code id
    pub code_id: u64,
    /// Tracking contract admin
    pub admin: String,
    /// Token factory module address
    pub token_factory_addr: String,
    /// Tracker contract address
    pub tracker_addr: String,
}

/// The structure returned as part of set_data when staking or unstaking
#[cw_serde]
pub struct StakingResponse {
    /// The ASTRO denom
    pub astro_amount: Uint128,
    /// The xASTRO denom
    pub xastro_amount: Uint128,
}
