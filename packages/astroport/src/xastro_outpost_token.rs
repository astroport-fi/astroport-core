use cosmwasm_schema::{cw_serde, QueryResponses};

use cosmwasm_std::{QuerierWrapper, StdResult, Uint128, Uint64};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllowanceResponse, BalanceResponse,
    DownloadLogoResponse, MarketingInfoResponse, MinterResponse, TokenInfoResponse,
};

/// This enum describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Balance returns the current balance of a given address, 0 if unset.
    #[returns(BalanceResponse)]
    Balance { address: String },
    /// BalanceAt returns balance of the given address at the given timestamp in seconds, 0 if unset.
    #[returns(BalanceResponse)]
    BalanceAt { address: String, timestamp: Uint64 },
    /// TotalSupplyAt returns the total token supply at the given timestamp in seconds.
    #[returns(Uint128)]
    TotalSupplyAt { timestamp: Uint64 },
    /// TokenInfo returns the contract's metadata - name, decimals, supply, etc.
    #[returns(TokenInfoResponse)]
    TokenInfo {},
    /// Returns who can mint xASTRO and the hard cap on maximum tokens after minting.
    #[returns(Option<MinterResponse>)]
    Minter {},
    /// Allowance returns an amount of tokens the spender can spend from the owner account, 0 if unset.
    #[returns(AllowanceResponse)]
    Allowance { owner: String, spender: String },
    /// AllAllowances returns all the allowances this token holder has approved. Supports pagination.
    #[returns(AllAllowancesResponse)]
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// AllAccounts returns all the accounts that have xASTRO balances. Supports pagination.
    #[returns(AllAccountsResponse)]
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns marketing related contract metadata:
    /// - description, logo, project url, etc.
    #[returns(MarketingInfoResponse)]
    MarketingInfo {},
    /// Downloads embeded logo data (if stored on chain). Errors if no logo data was stored for this contract.
    #[returns(DownloadLogoResponse)]
    DownloadLogo {},
}

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {}

/// Queries current user's voting power from the xASTRO contract by timestamp.
///
/// * **user** staker for which we calculate the voting power at a specific time.
///
/// * **timestamp** timestamp at which we fetch the staker's voting power.
pub fn get_voting_power_at_time(
    querier: &QuerierWrapper,
    xastro_addr: impl Into<String>,
    user: impl Into<String>,
    timestamp: impl Into<Uint64>,
) -> StdResult<Uint128> {
    let response: BalanceResponse = querier.query_wasm_smart(
        xastro_addr,
        &QueryMsg::BalanceAt {
            address: user.into(),
            timestamp: timestamp.into(),
        },
    )?;
    Ok(response.balance)
}
