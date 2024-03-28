use cosmwasm_schema::write_api;
use cw1_whitelist::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use neutron_sdk::bindings::msg::NeutronMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg<NeutronMsg>
    }
}
