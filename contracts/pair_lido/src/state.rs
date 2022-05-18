// SPDX-License-Identifier: GPL-3.0-only
// Copyright Astroport
// Copyright Lido

use astroport::asset::{Asset, AssetInfo, PairInfo};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// General pair information (e.g pair type)
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,

    /// The last time block
    pub block_time_last: u64,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    /// the Lido contract addresses
    pub hub_addr: Addr,
    pub stluna_addr: Addr,
    pub bluna_addr: Addr,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Describes additional params for user's swap request for processing in reply handler
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapRequestInfo {
    pub ask_asset_info: AssetInfo,
    pub offer_asset_info: Asset,
    pub belief_price: Option<Decimal>,
    pub max_spread: Option<Decimal>,
}

/// ## Description
/// Describes user's swap request for processing in reply handler
/// (<USER_ADDR>, <SwapRequestInfo>)
pub type SwapRequest = (Addr, SwapRequestInfo);

/// ## Description
/// Stores addr of recipient who should get converted tokens and address of converted tokens contract
pub const SWAP_REQUEST: Item<SwapRequest> = Item::new("swap_recipient");
