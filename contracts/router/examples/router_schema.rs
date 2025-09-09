use astroport::router::{ExecuteMsg, QueryMsg};
use cosmwasm_schema::write_api;
use cosmwasm_std::Empty;

fn main() {
    write_api! {
        instantiate: Empty,
        query: QueryMsg,
        execute: ExecuteMsg,
    }
}
