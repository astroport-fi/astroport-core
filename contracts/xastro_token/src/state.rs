use cosmwasm_std::{Addr, Env, Order, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Map, SnapshotMap, Strategy, U64Key};

/// ## Description
/// Contains balances at every block.
pub const BALANCES: SnapshotMap<&Addr, Uint128> = SnapshotMap::new(
    "balance",
    "balance__checkpoints",
    "balance__changelog",
    Strategy::EveryBlock,
);

/// ## Description
/// Contains saved total supply history.
pub const TOTAL_SUPPLY_HISTORY: Map<U64Key, Uint128> = Map::new("total_supply_history");

pub fn capture_total_supply_history(
    storage: &mut dyn Storage,
    env: &Env,
    total_supply: Uint128,
) -> StdResult<()> {
    TOTAL_SUPPLY_HISTORY.save(storage, U64Key::new(env.block.height), &total_supply)
}

pub fn get_total_supply_at(storage: &dyn Storage, block: u64) -> StdResult<Uint128> {
    // Look for the last value recorded before the current block (if none then value is zero)
    let prefix = TOTAL_SUPPLY_HISTORY.prefix(());
    let end = Bound::inclusive(U64Key::new(block));
    let last_value_up_to_block = prefix
        .range(storage, None, Some(end), Order::Descending)
        .next();

    if let Some(value) = last_value_up_to_block {
        let (_, v) = value?;
        return Ok(v);
    }

    Ok(Uint128::zero())
}
