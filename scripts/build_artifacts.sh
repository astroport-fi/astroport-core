#!/usr/bin/env bash

set -e
set -o pipefail

U="cosmwasm"
V="0.13.0"

M=$(uname -m)
#M="x86_64" # Force Intel arch

A="linux/${M/x86_64/amd64}"
S=${M#x86_64}
S=${S:+-$S}

docker run --platform $A --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  $U/workspace-optimizer$S:$V

if [ "$arch" = "arm64" ]; then
    for file in $projectPath/*-aarch64.wasm; do
        mv "$file" "${file%-aarch64.wasm}.wasm"
    done
fi