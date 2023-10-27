use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Env, Order, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::Item;
use cw_storage_plus::{Bound, Map, SnapshotMap, Strategy};

#[cw_serde]
pub struct Config {
    pub tokenfactory_module_address: String,
    pub tracked_denom: String,
}

pub const CONFIG: Item<Config> = Item::new("config");

/// Contains snapshotted balances at every block.
pub const BALANCES: SnapshotMap<&String, Uint128> = SnapshotMap::new(
    "balance",
    "balance__checkpoints",
    "balance__changelog",
    Strategy::EveryBlock,
);

/// Contains the history of the xASTRO total supply.
pub const TOTAL_SUPPLY_HISTORY: Map<u64, Uint128> = Map::new("total_supply_history");

// /// Snapshots the total token supply at current timestamp.
// ///
// /// * **total_supply** current token total supply.
// pub fn capture_total_supply_history(
//     storage: &mut dyn Storage,
//     env: &Env,
//     total_supply: Uint128,
// ) -> StdResult<()> {
//     TOTAL_SUPPLY_HISTORY.save(storage, env.block.time.seconds(), &total_supply)
// }

// /// Returns the total token supply at the given timestamp.
// pub fn get_total_supply_at(storage: &dyn Storage, timestamp: Uint64) -> StdResult<Uint128> {
//     // Look for the last value recorded before the current timestamp (if none then value is zero)
//     let end = Bound::inclusive(timestamp);
//     let last_value_up_to_second = TOTAL_SUPPLY_HISTORY
//         .range(storage, None, Some(end), Order::Descending)
//         .next();

//     if let Some(value) = last_value_up_to_second {
//         let (_, v) = value?;
//         return Ok(v);
//     }

//     Ok(Uint128::zero())
// }

// /// Checks that the sender is the minter. This is to authorise minting and burning of tokens
// pub fn check_sender_is_minter(sender: &Addr, config: &TokenInfo) -> Result<(), ContractError> {
//     if let Some(ref mint_data) = config.mint {
//         if mint_data.minter != sender {
//             return Err(ContractError::Unauthorized {});
//         }
//     } else {
//         return Err(ContractError::Unauthorized {});
//     }
//     Ok(())
// }
