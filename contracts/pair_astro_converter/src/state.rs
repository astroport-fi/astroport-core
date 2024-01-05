use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

use astroport::asset::PairInfo;

/// This structure stores the main config parameters for a constant product pair contract.
#[cw_serde]
pub struct Config {
    /// General pair information (e.g pair type)
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// ASTRO converter contract address
    pub converter_contract: Addr,
}

/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");
