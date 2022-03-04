use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20::{Cw20Coin, MinterResponse};

/// This structure describes the parameters used for creating a xASTRO token contract.
#[derive(Serialize, Deserialize, JsonSchema)]
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
}

/// This enum describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Balance returns the current balance of a given address, 0 if unset.
    Balance { address: String },
    /// BalanceAt returns balance of the given address at the given block, 0 if unset.
    BalanceAt { address: String, block: u64 },
    /// TotalSupplyAt returns the total token supply at the given block.
    TotalSupplyAt { block: u64 },
    /// TokenInfo returns the contract's metadata - name, decimals, supply, etc.
    TokenInfo {},
    /// Returns who can mint xASTRO and the hard cap on maximum tokens after minting.
    Minter {},
    /// Allowance returns an amount of tokens the spender can spend from the owner account, 0 if unset.
    Allowance { owner: String, spender: String },
    /// AllAllowances returns all the allowances this token holder has approved. Supports pagination.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// AllAccounts returns all the accounts that have xASTRO balances. Supports pagination.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns marketing related contract metadata:
    /// - description, logo, project url, etc.
    MarketingInfo {},
    /// Downloads embeded logo data (if stored on chain). Errors if no logo data was stored for this contract.
    DownloadLogo {},
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, JsonSchema)]
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
/// ## Params
/// * **name** is an object of type [`str`]. It is the token name to check.
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

/// Checks the validity of a token's symbol.
/// ## Params
/// * **symbol** is an object of type [`str`]. It is the token symbol to check.
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
