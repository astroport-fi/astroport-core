use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

use astroport::asset::{Asset, PairInfo};

/// This structure stores the main config parameters for a constant product pair contract.
#[cw_serde]
pub struct Config {
    /// General pair information (e.g pair type)
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// SuperVault contract address
    pub vault_addr: Addr,
    /// The LP token denom used in the SuperVault
    pub vault_lp_denom: String,
    /// Asset denoms
    pub denoms: [String; 2],
}

/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct ProvideTmpData {
    pub assets: Vec<Asset>,
    pub receiver: Addr,
    pub auto_stake: bool,
    pub min_lp_to_receive: Option<Uint128>,
}

pub const PROVIDE_TMP_DATA: Item<ProvideTmpData> = Item::new("provide_tmp_data");

#[cw_serde]
pub struct WithdrawTmpData {
    pub lp_amount: Uint128,
    pub receiver: Addr,
    pub min_assets_to_receive: Option<Vec<Asset>>,
}

pub const WITHDRAW_TMP_DATA: Item<WithdrawTmpData> = Item::new("withdraw_tmp_data");
