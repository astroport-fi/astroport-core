use cosmwasm_std::{Addr, Env, Order, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Map, SnapshotMap, Strategy};

/// Contains snapshotted coins balances at every block.
pub const BALANCES: SnapshotMap<&Addr, Uint128> = SnapshotMap::new(
    "balance",
    "balance__checkpoints",
    "balance__changelog",
    Strategy::EveryBlock,
);

/// Contains the history of the xASTRO total supply.
pub const TOTAL_SUPPLY_HISTORY: Map<u64, Uint128> = Map::new("total_supply_history");

/// Snapshots the total token supply at current block.
///
/// * **total_supply** current token total supply.
pub fn capture_total_supply_history(
    storage: &mut dyn Storage,
    env: &Env,
    total_supply: Uint128,
) -> StdResult<()> {
    TOTAL_SUPPLY_HISTORY.save(storage, env.block.height, &total_supply)
}

/// Returns the total token supply at the given block.
pub fn get_total_supply_at(storage: &dyn Storage, block: u64) -> StdResult<Uint128> {
    // Look for the last value recorded before the current block (if none then value is zero)
    let end = Bound::inclusive(block);
    let last_value_up_to_block = TOTAL_SUPPLY_HISTORY
        .range(storage, None, Some(end), Order::Descending)
        .next();

    if let Some(value) = last_value_up_to_block {
        let (_, v) = value?;
        return Ok(v);
    }

    Ok(Uint128::zero())
}
