#!/usr/bin/env bash

set -e

declare SCHEMAS
declare ROOT_DIR
ROOT_DIR="${PWD%/*}"

SCHEMAS="$(cargo metadata --no-deps --locked --manifest-path "$ROOT_DIR/Cargo.toml" --format-version 1 |
  jq -r --arg contracts "$ROOT_DIR/contracts" \
    '.packages[]
   | select(.manifest_path | startswith($contracts))
   | {
    name: .name,
    example: .targets[] | select(.kind==["example"]) | .name
    }' |
  jq -s)"

node --loader ts-node/esm codegen.ts <<<"$SCHEMAS"