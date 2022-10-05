use crate::state::PAIR_CONFIGS;
use astroport::factory::{PairConfig, PairType};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdError, Storage};
use cw_storage_plus::{Item, Map};

/// This structure describes a contract migration message.
#[cw_serde]
pub struct MigrationMsgV100 {
    /// CW1 whitelist contract code ID used to store 3rd party staking rewards
    pub whitelist_code_id: u64,
}

/// This structure holds the main parameters for the factory contract.
#[cw_serde]
pub struct ConfigV100 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Generator contract address
    pub generator_address: Option<Addr>,
    /// Contract address to send governance fees to (the Maker contract)
    pub fee_address: Option<Addr>,
}

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");

/// This structure describes a pair's configuration.
#[cw_serde]
pub struct PairConfigV100 {
    /// Pair contract code ID that's used to create new pairs of this type
    pub code_id: u64,
    /// The pair type (e.g XYK, stable)
    pub pair_type: PairType,
    /// The total amount of fees charged for the swap
    pub total_fee_bps: u16,
    /// The amount of fees that go to the Maker contract
    pub maker_fee_bps: u16,
    /// We disable pair configs instead of removing them. If a pair type is disabled,
    /// new pairs cannot be created, but existing ones can still function properly
    pub is_disabled: Option<bool>,
}

pub const PAIR_CONFIGS_V100: Map<String, PairConfigV100> = Map::new("pair_configs");

pub fn migrate_pair_configs_to_v120(storage: &mut dyn Storage) -> Result<(), StdError> {
    let keys = PAIR_CONFIGS_V100
        .keys(storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<Result<Vec<String>, StdError>>()?;

    for key in keys {
        let pair_configs_v100 = PAIR_CONFIGS_V100.load(storage, key.clone())?;
        let pair_config = PairConfig {
            code_id: pair_configs_v100.code_id,
            pair_type: pair_configs_v100.pair_type,
            total_fee_bps: pair_configs_v100.total_fee_bps,
            maker_fee_bps: pair_configs_v100.maker_fee_bps,
            is_disabled: pair_configs_v100.is_disabled.unwrap_or(false),
            is_generator_disabled: false,
        };
        PAIR_CONFIGS.save(storage, key, &pair_config)?;
    }

    Ok(())
}
