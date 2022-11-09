#!/usr/bin/env bash

set -e
set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

docker run -v "$projectPath":/code \
  --mount type=volume,source="$(basename "$projectPath")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.9


maximum_size=700

for artifact in artifacts/*.wasm; do
  artifactsize=$(du -k "$artifact" | cut -f 1)
  if [ "$artifactsize" -gt $maximum_size ]; then
    echo "Artifact file size exceeded: $artifact"
    echo "Artifact size: $artifactsize"
    echo "Max size: $maximum_size"
    exit 1
  fi
done