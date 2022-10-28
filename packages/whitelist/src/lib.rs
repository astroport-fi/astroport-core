use cosmwasm_schema::cw_serde;

pub use cw1_whitelist::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {}
