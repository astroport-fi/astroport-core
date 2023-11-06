use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Deps, Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Item, Map};

use astroport::{common::OwnershipProposal, cw20_tf_converter::Config};

use crate::error::ContractError;

/// Stores the contract config
pub const CONFIG: Item<Config> = Item::new("config");
