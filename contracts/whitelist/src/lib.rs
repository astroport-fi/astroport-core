#![cfg(not(tarpaulin_include))]
pub mod contract;
/// Exclusively to obtain IBC port and bypass Neutron IbcTransfer callbacks limitation.
/// Whitelist doesn't have IBC features.
pub mod ibc;
