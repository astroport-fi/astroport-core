use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, SnapshotMap};

use astroport::asset::AssetInfo;
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
