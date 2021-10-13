#!/usr/bin/env bash

set -e

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

cd "$projectPath/scripts" && node --loader ts-node/esm create_astro.ts
cd "$projectPath/scripts" && node --loader ts-node/esm deploy.ts
