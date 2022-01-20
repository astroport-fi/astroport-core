use cw_storage_plus::{Bound, Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Deps, Order};

use astroport::asset::AssetInfo;

use astroport::common::OwnershipProposal;
use astroport::factory::PairConfig;

/// ## Description
/// This structure describes the main control config of factory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The Contract address that used for controls settings for factory, pools and tokenomics contracts
    pub owner: Addr,
    /// CW20 token contract code identifier
    pub token_code_id: u64,
    /// contract address that used for auto_stake from pools
    pub generator_address: Option<Addr>,
    /// contract address to send fees to
    pub fee_address: Option<Addr>,
    /// cw1 whitelist contract code id used to store 3rd party rewards in pools
    pub whitelist_code_id: u64,
}

/// ## Description
/// This is an intermediate structure for storing the key of a pair and used in reply of submessage.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TmpPairInfo {
    pub pair_key: Vec<u8>,
}

/// Saves a key of pair
pub const TMP_PAIR_INFO: Item<TmpPairInfo> = Item::new("tmp_pair_info");

/// Saves factory settings
pub const CONFIG: Item<Config> = Item::new("config");

/// Saves created pairs
pub const PAIRS: Map<&[u8], Addr> = Map::new("pair_info");

/// ## Description
/// Calculates key of pair from the specified parameters in the `asset_infos` variable.
/// ## Params
/// `asset_infos` it is array with two items the type of [`AssetInfo`].
pub fn pair_key(asset_infos: &[AssetInfo; 2]) -> Vec<u8> {
    let mut asset_infos = asset_infos.to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat()
}

/// Saves the settings of the created pairs
pub const PAIR_CONFIGS: Map<String, PairConfig> = Map::new("pair_configs");

//settings for pagination
/// The maximum limit for reading pairs from a [`PAIRS`]
const MAX_LIMIT: u32 = 30;

/// The default limit for reading pairs from a [`PAIRS`]
const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Reads pairs from the [`PAIRS`] according to the specified parameters in `start_after` and `limit` variables.
/// Otherwise, it returns the default number of pairs.
/// ## Params
/// `start_after` is a [`Option`] type. Sets the item to start reading from.
///
/// `limit` is a [`Option`] type. Sets the number of items to be read.
pub fn read_pairs(
    deps: Deps,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> Vec<Addr> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after).map(Bound::exclusive);

    PAIRS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_, pair_addr) = item.unwrap();
            pair_addr
        })
        .collect()
}

// this will set the first key after the provided key, by appending a 1 byte
/// ## Description
/// Calculates the key of the pair from which to start reading.
/// ## Params
/// `start_after` is an [`Option`] type that accepts two [`AssetInfo`] elements.
fn calc_range_start(start_after: Option<[AssetInfo; 2]>) -> Option<Vec<u8>> {
    start_after.map(|asset_infos| {
        let mut asset_infos = asset_infos.to_vec();
        asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let mut v = [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()]
            .concat()
            .as_slice()
            .to_vec();
        v.push(1);
        v
    })
}

pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
