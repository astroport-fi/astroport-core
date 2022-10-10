use astroport::pair::InstantiateMsg;
use astroport::pair_stable_bluna::{ExecuteMsg, MigrateMsg, QueryMsg};
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
