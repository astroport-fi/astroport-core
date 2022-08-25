#!/usr/bin/env bash

set -e

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

cd "$projectPath/scripts" && node --loader ts-node/esm deploy_core.ts
cd "$projectPath/scripts" && node --loader ts-node/esm deploy_pools.ts
