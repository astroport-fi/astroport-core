use astroport::pair::{ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport::pair_xyk_sale_tax::MigrateMsg;
use cosmwasm_schema::write_api;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg,
    }
}
