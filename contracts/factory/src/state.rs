use cosmwasm_std::{Addr, Api};
use cw_storage_plus::{index_list, IndexedMap, Item, Map, MultiIndex, UniqueIndex};
use itertools::Itertools;

use astroport::asset::{AssetInfo, PairInfo};
use astroport::common::OwnershipProposal;
use astroport::factory::{Config, PairConfig};

use crate::error::ContractError;

/// Saves factory settings
pub const CONFIG: Item<Config> = Item::new("config");

#[index_list(PairInfo)]
pub struct PairIndexes<'a> {
    pub assets_ix: MultiIndex<'a, Vec<u8>, PairInfo, Addr>,
    pub lp_tokens_ix: UniqueIndex<'a, String, PairInfo, Addr>,
}

/// Index over all pairs
pub const PAIRS: IndexedMap<&Addr, PairInfo, PairIndexes> = IndexedMap::new(
    "p",
    PairIndexes {
        assets_ix: MultiIndex::new(|_, pi| pair_key(&pi.asset_infos), "p", "as"),
        lp_tokens_ix: UniqueIndex::new(|pi| pi.liquidity_token.clone(), "lp"),
    },
);

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
/// The default limit for reading pairs from [`PAIRS`]
pub const DEFAULT_LIMIT: u32 = 10;

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
