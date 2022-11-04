use cosmwasm_schema::cw_serde;

/// This structure stores pool's params.
/// Declare here pair params
#[cw_serde]
pub struct Params {}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}
