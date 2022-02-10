use crate::state::PAIR_CONFIGS;
use astroport::factory::{PairConfig, PairType};
use cosmwasm_std::{Addr, StdError, Storage};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrationMsgV100 {
    /// cw1 whitelist contract code id used to store 3rd party rewards in pools
    pub whitelist_code_id: u64,
}

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

/// This structure describes a configuration of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairConfigV110 {
    /// pair contract code ID which are allowed to create pair
    pub code_id: u64,
    /// the type of pair available in [`PairType`]
    pub pair_type: PairType,
    /// a pair total fees bps
    pub total_fee_bps: u16,
    /// a pair fees bps
    pub maker_fee_bps: u16,
    /// We disable pair configs instead of removing them. If it is disabled, new pairs cannot be
    /// created, but existing ones can still obtain proper settings, such as fee amounts
    pub is_disabled: Option<bool>,
}

pub const PAIR_CONFIGSV110: Map<String, PairConfigV110> = Map::new("pair_configs");

pub fn migrate_pair_configs_to_v120(storage: &mut dyn Storage) -> Result<(), StdError> {
    let keys = PAIR_CONFIGSV110
        .keys(storage, None, None, cosmwasm_std::Order::Ascending {})
        .map(|v| String::from_utf8(v).map_err(StdError::from))
        .collect::<Result<Vec<String>, StdError>>()?;

    for key in keys {
        let pair_configs_v110 = PAIR_CONFIGSV110.load(storage, key.clone())?;
        let pair_config = PairConfig {
            code_id: pair_configs_v110.code_id,
            pair_type: pair_configs_v110.pair_type,
            total_fee_bps: pair_configs_v110.total_fee_bps,
            maker_fee_bps: pair_configs_v110.maker_fee_bps,
            is_disabled: pair_configs_v110.is_disabled.unwrap_or(false),
            is_generator_disabled: false,
        };
        PAIR_CONFIGS.save(storage, key, &pair_config)?;
    }

    Ok(())
}
