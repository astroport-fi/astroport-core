pub mod contract;
pub mod math;
pub mod msg;
pub mod state;

#[cfg(test)]
mod testing;

#[cfg(test)]
mod mock_querier;

#[cfg(all(target_arch = "wasm32", not(feature = "library")))]
cosmwasm_std::create_entry_points_with_migration!(contract);
