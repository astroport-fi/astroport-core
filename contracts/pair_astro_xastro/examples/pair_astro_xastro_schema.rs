use cosmwasm_schema::write_api;

use astroport::pair::InstantiateMsg;
use astroport::pair_bonded::{ExecuteMsg, QueryMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
    }
}
