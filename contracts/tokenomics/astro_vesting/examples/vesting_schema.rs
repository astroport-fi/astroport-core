use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use astroport::astro_vesting::msg::{
    AllocationResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, SimulateWithdrawResponse,
};
use astroport::astro_vesting::{AllocationParams, AllocationStatus, Schedule};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(ReceiveMsg), &out_dir);
    export_schema(&schema_for!(SimulateWithdrawResponse), &out_dir);
    export_schema(&schema_for!(AllocationResponse), &out_dir);
    export_schema(&schema_for!(Schedule), &out_dir);
    export_schema(&schema_for!(AllocationParams), &out_dir);
    export_schema(&schema_for!(AllocationStatus), &out_dir);
}
