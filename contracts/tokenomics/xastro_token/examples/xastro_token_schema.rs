use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema_with_title, remove_schemas, schema_for};

use astroport::xastro_token::{InstantiateMsg, QueryMsg};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllowanceResponse, BalanceResponse, MinterResponse,
    TokenInfoResponse,
};
use cw20_base::msg::ExecuteMsg;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");
    export_schema_with_title(
        &schema_for!(AllowanceResponse),
        &out_dir,
        "AllowanceResponse",
    );
    export_schema_with_title(&schema_for!(BalanceResponse), &out_dir, "BalanceResponse");
    export_schema_with_title(
        &schema_for!(TokenInfoResponse),
        &out_dir,
        "TokenInfoResponse",
    );
    export_schema_with_title(&schema_for!(MinterResponse), &out_dir, "MinterResponse");
    export_schema_with_title(
        &schema_for!(AllAllowancesResponse),
        &out_dir,
        "AllAllowancesResponse",
    );
    export_schema_with_title(
        &schema_for!(AllAccountsResponse),
        &out_dir,
        "AllAccountsResponse",
    );
}
