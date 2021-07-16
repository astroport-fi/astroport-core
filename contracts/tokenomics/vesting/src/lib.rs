pub mod contract;
pub mod state;
pub mod error;

#[cfg(test)]
mod testing;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points!(contract);
