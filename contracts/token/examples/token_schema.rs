use cosmwasm_schema::write_api;

use ap_token::{ExecuteMsg, InstantiateMsg, MigrateMsg};
use cw20_base::msg::QueryMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
