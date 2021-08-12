use std::env::current_dir;
use std::fs::create_dir_all;

use astroport_governance::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, StateResponse,
};
use astroport_governance::state::{LockedBalance, Proposal};
use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(StateResponse), &out_dir);
    export_schema(&schema_for!(Proposal), &out_dir);
    export_schema(&schema_for!(LockedBalance), &out_dir);
    export_schema(&schema_for!(ConfigResponse), &out_dir);
}
