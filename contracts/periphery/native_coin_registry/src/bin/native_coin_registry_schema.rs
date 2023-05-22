use astroport::native_coin_registry::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
        migrate: MigrateMsg
    }
}
