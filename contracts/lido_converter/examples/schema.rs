// SPDX-License-Identifier: GPL-3.0-only
// Copyright Astroport
// Copyright Lido

use std::env::current_dir;
use std::fs::create_dir_all;

use astroport::pair::{Cw20HookMsg, ExecuteMsg, QueryMsg};
use astroport_stluna_bluna_lido_converter::msgs::InstantiateMsg;
use astroport_stluna_bluna_lido_converter::state::ConfigResponse;
use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(Cw20HookMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(ConfigResponse), &out_dir);
}
