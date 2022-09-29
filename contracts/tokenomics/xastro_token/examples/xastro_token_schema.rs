use astroport::xastro_token::{InstantiateMsg, QueryMsg};
use cosmwasm_schema::write_api;
use cw20_base::msg::ExecuteMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg
    }
}
