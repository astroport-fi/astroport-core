use astroport::factory::PairType;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// This structure describes a contract migration message.
#[cw_serde]
pub struct MigrationMsg {
    /// CW1 whitelist contract code ID used to store 3rd party staking rewards
    pub whitelist_code_id: u64,
    /// The address of the contract that contains native coins with their precisions
    pub coin_registry_address: String,
}

/// This structure holds the main parameters for the factory contract.
#[cw_serde]
pub struct ConfigV120 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// Generator contract address
    pub generator_address: Option<Addr>,
    /// Contract address to send governance fees to (the Maker contract)
    pub fee_address: Option<Addr>,
    /// CW1 whitelist contract code id used to store 3rd party generator staking rewards
    pub whitelist_code_id: u64,
}

pub const CONFIG_V120: Item<ConfigV120> = Item::new("config");

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
