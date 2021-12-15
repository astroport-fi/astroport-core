use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20::{Cw20Coin, MinterResponse};

/// ## Description
/// This structure describes the basic settings for creating a token contract.
/// TokenContract InstantiateMsg
#[derive(Serialize, Deserialize, JsonSchema)]
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
