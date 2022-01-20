use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the main control config of factory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    /// The Contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: Addr,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// contract address that used for auto_stake from pools
    pub generator_address: Option<Addr>,
    /// contract address to send fees to
    pub fee_address: Option<Addr>,
}

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");
