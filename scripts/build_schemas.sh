#!/usr/bin/env bash

set -e

TARGETS="$(cargo metadata --no-deps --locked --manifest-path "$PWD/Cargo.toml" --format-version 1 |
  jq -r --arg contracts "$PWD/contracts" -r \
    '.packages[]
    | select(.manifest_path | startswith($contracts))
    | .name + " " + (.targets[] | select(.kind==["example"]) | .name)')"

rm -rf schemas

while read -r contract schema_builder; do
  if [[ ! "$schema_builder" =~ "_schema" ]]; then
    echo "Skipping example $schema_builder"
    continue
  fi
  echo "Building $contract $schema_builder"
  cargo run --locked --example "$schema_builder"

  mkdir -p "schemas/$contract"
  mv "$PWD/schema/"* "$PWD/schemas/$contract/"
done <<<"$TARGETS"

rmdir schema
