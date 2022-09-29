use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Bound, Item, Map};

use cosmwasm_std::{Addr, Deps, Order};

use astroport::asset::AssetInfo;

use astroport::common::OwnershipProposal;
use astroport::factory::PairConfig;

/// ## Description
/// This structure holds the main contract parameters.
#[cw_serde]
pub struct Config {
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

/// ## Description
/// This is an intermediate structure for storing a pair's key. It is used in a submessage response.
#[cw_serde]
pub struct TmpPairInfo {
    pub pair_key: Vec<u8>,
}

/// Saves a pair's key
pub const TMP_PAIR_INFO: Item<TmpPairInfo> = Item::new("tmp_pair_info");

/// Saves factory settings
pub const CONFIG: Item<Config> = Item::new("config");

/// Saves created pairs (from olders to latest)
pub const PAIRS: Map<&[u8], Addr> = Map::new("pair_info");

/// ## Description
/// Calculates a pair key from the specified parameters in the `asset_infos` variable.
/// ## Params
/// `asset_infos` is an array with two items of type [`AssetInfo`].
pub fn pair_key(asset_infos: &[AssetInfo; 2]) -> Vec<u8> {
    let mut asset_infos = asset_infos.to_vec();
    asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat()
}

/// Saves pair type configurations
pub const PAIR_CONFIGS: Map<String, PairConfig> = Map::new("pair_configs");

/// ## Pagination settings
/// The maximum limit for reading pairs from [`PAIRS`]
const MAX_LIMIT: u32 = 30;
/// The default limit for reading pairs from [`PAIRS`]
const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Reads pairs from the [`PAIRS`] vector according to the `start_after` and `limit` variables.
/// Otherwise, it returns the default number of pairs, starting from the oldest one.
/// ## Params
/// `start_after` is the pair from which the function starts to fetch results. It is an [`Option`].
///
/// `limit` is the number of items to retreive. It is an [`Option`].
pub fn read_pairs(
    deps: Deps,
    start_after: Option<[AssetInfo; 2]>,
    limit: Option<u32>,
) -> Vec<Addr> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    if let Some(data) = calc_range_start(start_after) {
        PAIRS
            .range(
                deps.storage,
                Some(Bound::exclusive(data.as_slice())),
                None,
                Order::Ascending,
            )
            .take(limit)
            .map(|item| {
                let (_, pair_addr) = item.unwrap();
                pair_addr
            })
            .collect()
    } else {
        PAIRS
            .range(deps.storage, None, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (_, pair_addr) = item.unwrap();
                pair_addr
            })
            .collect()
    }
}

/// ## Description
/// Calculates the key of a pair from which to start reading data.
/// ## Params
/// `start_after` is an [`Option`] type that accepts two [`AssetInfo`] elements.
/// It is the token pair which we use to determine the start index for a range when returning data for multiple pairs
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
