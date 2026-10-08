use astroport::pair::{ExecuteMsg, InstantiateMsg};
use astroport::pair_concentrated::QueryMsg;
use astroport_pair_concentrated::contract::MigrateMsg;
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
