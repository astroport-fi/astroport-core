use cosmwasm_schema::write_api;

use astroport::tokenfactory_tracker::{InstantiateMsg, QueryMsg, SudoMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        sudo: SudoMsg,
    }
}
