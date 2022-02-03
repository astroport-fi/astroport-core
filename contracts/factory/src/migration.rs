use astroport::factory::PairType;
use cw_storage_plus::Map;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the main control config of factory.
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
