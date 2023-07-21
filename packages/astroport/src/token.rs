use cosmwasm_schema::cw_serde;

use cosmwasm_std::{StdError, StdResult, Uint128};
pub use cw20::{
    BalanceResponse, Cw20Coin, Cw20ExecuteMsg as ExecuteMsg, Cw20QueryMsg as QueryMsg, Logo,
    MinterResponse,
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

/// This structure describes the parameters used for creating a token contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// The amount of decimals the token has
    pub decimals: u8,
    /// Initial token balances
    pub initial_balances: Vec<Cw20Coin>,
    /// Minting controls specified in a [`MinterResponse`] structure
    pub mint: Option<MinterResponse>,
    /// the marketing info of type [`InstantiateMarketingInfo`]
    pub marketing: Option<InstantiateMarketingInfo>,
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

/// Checks the validity of the token name
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

/// Checks the validity of the token symbol
pub fn is_valid_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if bytes.len() < 3 || bytes.len() > 12 {
        return false;
    }
    for byte in bytes.iter() {
        if (*byte != 45)
            && (*byte < 47 || *byte > 57)
            && (*byte < 65 || *byte > 90)
            && (*byte < 97 || *byte > 122)
        {
            return false;
        }
    }
    true
}
