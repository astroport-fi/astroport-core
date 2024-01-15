use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// The contract owner address
    pub owner: String,
    /// The ASTRO token contract address
    pub deposit_token_denom: String,
    // The Code ID of contract used to track the TokenFactory token balances
    pub tracking_code_id: u64,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Deposits ASTRO in exchange for xASTRO
    Enter {},
    /// Burns xASTRO in exchange for ASTRO
    Leave {},
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns the contract configuration specified in a custom [`ConfigResponse`] structure
    #[returns(ConfigResponse)]
    Config {},
    #[returns(Uint128)]
    TotalShares {},
    #[returns(Uint128)]
    TotalDeposit {},
}

#[cw_serde]
pub struct ConfigResponse {
    /// The ASTRO denom
    pub deposit_denom: String,
    /// The xASTRO denom
    pub share_denom: String,
    // TODO: Comments
    pub share_tracking_address: String,
}

// The structure returned as part of set_data when staking or unstaking
#[cw_serde]
pub struct StakingResponse {
    /// The ASTRO denom
    pub astro_amount: Uint128,
    /// The xASTRO denom
    pub xastro_amount: Uint128,
}

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {}
