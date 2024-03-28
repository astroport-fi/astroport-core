use cosmwasm_schema::write_api;

use astroport::astro_converter::{ExecuteMsg, InstantiateMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
    }
}
