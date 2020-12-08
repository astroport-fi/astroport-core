use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use terraswap::{
    Asset, AssetInfo, FactoryHandleMsg, FactoryQueryMsg, InitHook, PairCw20HookMsg, PairHandleMsg,
    PairInfo, PairInitMsg, PairQueryMsg, TokenInitMsg,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(FactoryHandleMsg), &out_dir);
    export_schema(&schema_for!(FactoryQueryMsg), &out_dir);
    export_schema(&schema_for!(PairCw20HookMsg), &out_dir);
    export_schema(&schema_for!(PairHandleMsg), &out_dir);
    export_schema(&schema_for!(PairQueryMsg), &out_dir);
    export_schema(&schema_for!(PairInfo), &out_dir);
    export_schema(&schema_for!(PairInitMsg), &out_dir);
    export_schema(&schema_for!(TokenInitMsg), &out_dir);
    export_schema(&schema_for!(InitHook), &out_dir);
    export_schema(&schema_for!(Asset), &out_dir);
    export_schema(&schema_for!(AssetInfo), &out_dir);
}
