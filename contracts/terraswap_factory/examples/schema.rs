use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use cosmwasm_std::HandleResponse;

use terraswap::PairInfo;
use terraswap_factory::msg::{ConfigResponse, HandleMsg, InitMsg, QueryMsg, PairsResponse};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InitMsg), &out_dir);
    export_schema(&schema_for!(HandleMsg), &out_dir);
    export_schema(&schema_for!(HandleResponse), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(PairInfo), &out_dir);
    export_schema(&schema_for!(PairsResponse), &out_dir);
    export_schema(&schema_for!(ConfigResponse), &out_dir);
}
