use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use cosmwasm_std::HandleResponse;

use terraswap::{PairInfo, PairInitMsg};
use terraswap_pair::msg::{
    Cw20HookMsg, HandleMsg, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(PairInitMsg), &out_dir);
    export_schema(&schema_for!(HandleMsg), &out_dir);
    export_schema(&schema_for!(Cw20HookMsg), &out_dir);
    export_schema(&schema_for!(HandleResponse), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(PairInfo), &out_dir);
    export_schema(&schema_for!(PoolResponse), &out_dir);
    export_schema(&schema_for!(ReverseSimulationResponse), &out_dir);
    export_schema(&schema_for!(SimulationResponse), &out_dir);
}
