use astroport::asset::PairInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Storage, Uint128};
use cw_storage_plus::Item;

use crate::{
    error::ContractError,
    state::{Config, CONFIG},
};

pub(crate) fn add_asset_balances_tracking_flag(
    storage: &mut dyn Storage,
) -> Result<(), ContractError> {
    /// This structure stores the main config parameters for a constant product pair contract.
    #[cw_serde]
    pub struct ConfigUntilV130 {
        /// General pair information (e.g pair type)
        pub pair_info: PairInfo,
        /// The factory contract address
        pub factory_addr: Addr,
        /// The last timestamp when the pair contract update the asset cumulative prices
        pub block_time_last: u64,
        /// The last cumulative price for asset 0
        pub price0_cumulative_last: Uint128,
        /// The last cumulative price for asset 1
        pub price1_cumulative_last: Uint128,
    }

    /// Stores the config struct at the given key
    pub const CONFIG_UNTIL_V130: Item<ConfigUntilV130> = Item::new("config");

    let old_config = CONFIG_UNTIL_V130.load(storage)?;

    let new_config = Config {
        pair_info: old_config.pair_info,
        factory_addr: old_config.factory_addr,
        block_time_last: old_config.block_time_last,
        price0_cumulative_last: old_config.price0_cumulative_last,
        price1_cumulative_last: old_config.price1_cumulative_last,
        track_asset_balances: false,
    };

    CONFIG.save(storage, &new_config)?;

    Ok(())
}
