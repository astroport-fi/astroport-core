#!/usr/bin/env bash

set -e
set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

docker run --rm -v "$projectPath":/code \
  --mount type=volume,source="$(basename "$projectPath")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  ghcr.io/astroport-fi/rust-optimizer:v0.15.1-astroport
