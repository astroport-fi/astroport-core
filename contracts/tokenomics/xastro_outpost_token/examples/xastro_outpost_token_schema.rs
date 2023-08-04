use cosmwasm_schema::write_api;

use astroport::xastro_outpost_token::QueryMsg;
use cw20_base::msg::{ExecuteMsg, InstantiateMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg
    }
}
