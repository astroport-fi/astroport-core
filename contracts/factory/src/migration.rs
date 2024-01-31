use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdResult, Storage};
use cw_storage_plus::Map;

use astroport::factory::{PairConfig, PairType};

use crate::state::PAIR_CONFIGS;

#[cw_serde]
pub enum OldPairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Concentrated liquidity pair type
    Concentrated {},
    /// Custom pair type
    Custom(String),
}

/// This structure describes a pair's configuration.
#[cw_serde]
pub struct OldPairConfig {
    pub code_id: u64,
    pub pair_type: OldPairType,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
    pub is_disabled: bool,
    pub is_generator_disabled: bool,
}

pub const OLD_PAIR_CONFIGS: Map<String, OldPairConfig> = Map::new("pair_configs");

pub fn migrate_pair_configs(storage: &mut dyn Storage) -> StdResult<()> {
    let keys = OLD_PAIR_CONFIGS
        .keys(storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<StdResult<Vec<_>>>()?;

    for key in keys {
        let old_pair_config = OLD_PAIR_CONFIGS.load(storage, key.clone())?;

        let pair_type = match &old_pair_config.pair_type {
            OldPairType::Xyk {} => PairType::Xyk {},
            OldPairType::Stable {} => PairType::Stable {},
            OldPairType::Concentrated {} => PairType::Custom("concentrated".to_string()),
            OldPairType::Custom(pair_type) => PairType::Custom(pair_type.to_owned()),
        };

        let pair_config = PairConfig {
            code_id: old_pair_config.code_id,
            pair_type: pair_type.clone(),
            total_fee_bps: old_pair_config.total_fee_bps,
            maker_fee_bps: old_pair_config.maker_fee_bps,
            is_disabled: old_pair_config.is_disabled,
            is_generator_disabled: old_pair_config.is_generator_disabled,
            permissioned: false,
        };

        if key != pair_type.to_string() {
            PAIR_CONFIGS.remove(storage, key);
            PAIR_CONFIGS.save(storage, pair_type.to_string(), &pair_config)?;
        } else {
            PAIR_CONFIGS.save(storage, key, &pair_config)?;
        }
    }

    Ok(())
}
