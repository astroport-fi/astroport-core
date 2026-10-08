use cosmwasm_schema::write_api;

use astroport::staking::{ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport_staking::migrate::MigrateMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg,
    }
}
