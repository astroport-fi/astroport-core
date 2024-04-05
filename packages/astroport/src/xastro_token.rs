// TODO: DEPRECATE
use cosmwasm_schema::{cw_serde, QueryResponses};

use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllowanceResponse, BalanceResponse, Cw20Coin,
    DownloadLogoResponse, Logo, MarketingInfoResponse, MinterResponse, TokenInfoResponse,
};

/// This structure describes the marketing info settings such as project, description, and token logo.
#[cw_serde]
pub struct InstantiateMarketingInfo {
    /// The project name
    pub project: Option<String>,
    /// The project description
    pub description: Option<String>,
    /// The address of an admin who is able to update marketing info
    pub marketing: Option<String>,
    /// The token logo
    pub logo: Option<Logo>,
}

/// This structure describes the parameters used for creating a xASTRO token contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// The number of decimals the token has
    pub decimals: u8,
    /// Initial token balances
    pub initial_balances: Vec<Cw20Coin>,
    /// Token minting permissions
    pub mint: Option<MinterResponse>,
    /// the marketing info of type [`InstantiateMarketingInfo`]
    pub marketing: Option<InstantiateMarketingInfo>,
}

/// This enum describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Balance returns the current balance of a given address, 0 if unset.
    #[returns(BalanceResponse)]
    Balance { address: String },
    /// BalanceAt returns balance of the given address at the given block, 0 if unset.
    #[returns(BalanceResponse)]
    BalanceAt { address: String, block: u64 },
    /// TotalSupplyAt returns the total token supply at the given block.
    #[returns(Uint128)]
    TotalSupplyAt { block: u64 },
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

impl InstantiateMsg {
    pub fn get_cap(&self) -> Option<Uint128> {
        self.mint.as_ref().and_then(|v| v.cap)
    }

    pub fn validate(&self) -> StdResult<()> {
        // Check name, symbol, decimals
        if !is_valid_name(&self.name) {
            return Err(StdError::generic_err(
                "Name is not in the expected format (3-50 UTF-8 bytes)",
            ));
        }
        if !is_valid_symbol(&self.symbol) {
            return Err(StdError::generic_err(
                "Ticker symbol is not in expected format [a-zA-Z\\-]{3,12}",
            ));
        }
        if self.decimals > 18 {
            return Err(StdError::generic_err("Decimals must not exceed 18"));
        }
        Ok(())
    }
}

/// Checks the validity of a token's name.
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

/// Checks the validity of a token's symbol.
fn is_valid_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if bytes.len() < 3 || bytes.len() > 12 {
        return false;
    }
    for byte in bytes.iter() {
        if (*byte != 45) && (*byte < 65 || *byte > 90) && (*byte < 97 || *byte > 122) {
            return false;
        }
    }
    true
}
