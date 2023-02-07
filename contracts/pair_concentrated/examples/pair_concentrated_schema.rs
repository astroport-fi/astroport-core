use astroport::pair::{ExecuteMsg, InstantiateMsg, MigrateMsg};
use astroport::pair_concentrated::QueryMsg;
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
