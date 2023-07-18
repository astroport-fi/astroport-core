use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Deps, Order, StdResult};
use cw_storage_plus::{Bound, Item, Map};
use itertools::Itertools;

use crate::error::ContractError;
use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::factory::{Config, PairConfig};
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

/// Calculates a pair key from the specified parameters in the `asset_infos` variable.
///
/// `asset_infos` is an array with multiple items of type [`AssetInfo`].
pub fn pair_key(asset_infos: &[AssetInfo]) -> Vec<u8> {
    asset_infos
        .iter()
        .map(AssetInfo::as_bytes)
        .sorted()
        .flatten()
        .copied()
        .collect()
}

/// Saves pair type configurations
pub const PAIR_CONFIGS: Map<String, PairConfig> = Map::new("pair_configs");

/// ## Pagination settings
/// The maximum limit for reading pairs from [`PAIRS`]
const MAX_LIMIT: u32 = 30;
/// The default limit for reading pairs from [`PAIRS`]
const DEFAULT_LIMIT: u32 = 10;

/// Reads pairs from the [`PAIRS`] vector according to the `start_after` and `limit` variables.
/// Otherwise, it returns the default number of pairs, starting from the oldest one.
///
/// `start_after` is the pair from which the function starts to fetch results.
///
/// `limit` is the number of items to retrieve.
pub fn read_pairs(
    deps: Deps,
    start_after: Option<Vec<AssetInfo>>,
    limit: Option<u32>,
) -> StdResult<Vec<Addr>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    if let Some(start) = calc_range_start(start_after) {
        PAIRS
            .range(
                deps.storage,
                Some(Bound::exclusive(start.as_slice())),
                None,
                Order::Ascending,
            )
            .take(limit)
            .map(|item| {
                let (_, pair_addr) = item?;
                Ok(pair_addr)
            })
            .collect()
    } else {
        PAIRS
            .range(deps.storage, None, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (_, pair_addr) = item?;
                Ok(pair_addr)
            })
            .collect()
    }
}

/// Calculates the key of a pair from which to start reading data.
///
/// `start_after` is an [`Option`] type that accepts [`AssetInfo`] elements.
/// It is the token pair which we use to determine the start index for a range when returning data for multiple pairs
fn calc_range_start(start_after: Option<Vec<AssetInfo>>) -> Option<Vec<u8>> {
    start_after.map(|ref asset| {
        let mut key = pair_key(asset);
        key.push(1);
        key
    })
}

pub(crate) fn check_asset_infos(
    api: &dyn Api,
    asset_infos: &[AssetInfo],
) -> Result<(), ContractError> {
    if !asset_infos.iter().all_unique() {
        return Err(ContractError::DoublingAssets {});
    }

    asset_infos
        .iter()
        .try_for_each(|asset_info| asset_info.check(api))
        .map_err(Into::into)
}

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// This state key isn't used anymore but left for backward compatability with old pairs
pub const PAIRS_TO_MIGRATE: Item<Vec<Addr>> = Item::new("pairs_to_migrate");

#[cfg(test)]
mod tests {
    use astroport::asset::{native_asset_info, token_asset_info};

    use super::*;

    fn get_test_case() -> Vec<[AssetInfo; 2]> {
        vec![
            [
                native_asset_info("uluna".to_string()),
                native_asset_info("uusd".to_string()),
            ],
            [
                native_asset_info("uluna".to_string()),
                token_asset_info(Addr::unchecked("astro_token_addr")),
            ],
            [
                token_asset_info(Addr::unchecked("random_token_addr")),
                token_asset_info(Addr::unchecked("astro_token_addr")),
            ],
        ]
    }

    #[test]
    fn test_legacy_pair_key() {
        fn legacy_pair_key(asset_infos: &[AssetInfo; 2]) -> Vec<u8> {
            let mut asset_infos = asset_infos.to_vec();
            asset_infos.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

            [asset_infos[0].as_bytes(), asset_infos[1].as_bytes()].concat()
        }

        for asset_infos in get_test_case() {
            assert_eq!(legacy_pair_key(&asset_infos), pair_key(&asset_infos));
        }
    }

    #[test]
    fn test_legacy_start_after() {
        fn legacy_calc_range_start(start_after: Option<[AssetInfo; 2]>) -> Option<Vec<u8>> {
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

        for asset_infos in get_test_case() {
            assert_eq!(
                legacy_calc_range_start(Some(asset_infos.clone())),
                calc_range_start(Some(asset_infos.to_vec()))
            );
        }
    }
}
