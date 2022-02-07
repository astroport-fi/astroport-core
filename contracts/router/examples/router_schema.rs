use std::env::current_dir;
use std::fs::create_dir_all;

use astroport::router::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    SimulateSwapOperationsResponse,
};
use cosmwasm_schema::{export_schema_with_title, remove_schemas, schema_for};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(Cw20HookMsg), &out_dir, "Cw20HookMsg");
    export_schema_with_title(&schema_for!(MigrateMsg), &out_dir, "MigrateMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");
    export_schema_with_title(&schema_for!(ConfigResponse), &out_dir, "ConfigResponse");
    export_schema_with_title(
        &schema_for!(SimulateSwapOperationsResponse),
        &out_dir,
        "SimulateSwapOperationsResponse",
    );
}
