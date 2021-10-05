#!/usr/bin/env bash

set -e
set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

docker run --rm \
    --volume "$projectPath":/code \
    --volume "$(basename "$projectPath")-target":/code/target \
    --volume cargo-registry:/usr/local/cargo/registry \
    cosmwasm/workspace-optimizer:0.12.3
