use cosmwasm_std::{StdResult, Storage, Uint128};
use cw_storage_plus::{Item, SnapshotMap};

use astroport::asset::{Asset, AssetInfo};
use astroport::common::OwnershipProposal;
use astroport::observation::Observation;
use astroport_circular_buffer::CircularBuffer;
use astroport_pcl_common::state::Config;

/// Stores pool parameters and state.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Circular buffer to store trade size observations
pub const OBSERVATIONS: CircularBuffer<Observation> =
    CircularBuffer::new("observations_state", "observations_buffer");

/// Stores asset balances to query them later at any block height
pub const BALANCES: SnapshotMap<&AssetInfo, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    cw_storage_plus::Strategy::EveryBlock,
);

/// Exit-only mode: swaps, deposits and quotes are refused, and withdrawals return every asset
/// except this one, which stays in the pool. Only a migration sets it.
pub const EXIT_ONLY: Item<AssetInfo> = Item::new("exit_only");

/// Zeroes the withheld asset of a withdrawal refund when the pool is exit-only.
pub fn withhold(storage: &dyn Storage, refund: Vec<Asset>) -> StdResult<Vec<Asset>> {
    let withheld = EXIT_ONLY.may_load(storage)?;
    Ok(refund
        .into_iter()
        .map(|mut asset| {
            if withheld.as_ref() == Some(&asset.info) {
                asset.amount = Uint128::zero();
            }
            asset
        })
        .collect())
}
