use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20::{Cw20Coin, Logo, MinterResponse};

/// ## Description
/// This structure describes the marketing info settings such as project, description, and token logo.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
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

/// ## Description
/// This structure describes the basic settings for creating a token contract.
/// TokenContract InstantiateMsg
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct InstantiateMsg {
    /// the name
    pub name: String,
    /// the symbol
    pub symbol: String,
    /// the precision after the decimal point
    pub decimals: u8,
    /// the initial balance of token
    pub initial_balances: Vec<Cw20Coin>,
    /// the controls configs of type [`MinterResponse`]
    pub mint: Option<MinterResponse>,
    /// the marketing info of type [`InstantiateMarketingInfo`]
    pub marketing: Option<InstantiateMarketingInfo>,
}

/// ## Description
/// This enum describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the current balance of the given address, 0 if unset.
    Balance { address: String },
    /// Returns balance of the given address at the given block, 0 if unset.
    BalanceAt { address: String, block: u64 },
    /// Returns total supply at the given block.
    TotalSupplyAt { block: u64 },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    TokenInfo {},
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    Minter {},
    /// Returns how much spender can use from owner account, 0 if unset.
    Allowance { owner: String, spender: String },
    /// Returns all allowances this owner has approved. Supports pagination.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns all accounts that have balances. Supports pagination.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    MarketingInfo {},
    /// Downloads the mbeded logo data (if stored on chain). Errors if no logo data ftored for this
    /// contract.
    DownloadLogo {},
}

/// ## Description
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

/// ## Description
/// Checks the validity of the token name
/// ## Params
/// * **name** is the object of type [`str`]. the name to check
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

/// ## Description
/// Checks the validity of the token symbol
/// ## Params
/// * **symbol** is the object of type [`str`]. the symbol to check
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
