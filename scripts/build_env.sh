#!/usr/bin/env bash

set -e

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

artifactPath="$projectPath/artifacts"
if [ ! -d "$artifactPath" ]; then
    npm run build-artifacts
fi

terraLocalPath="${TERRA_LOCAL_PATH:-"$(dirname "$projectPath")/terra-local"}"
if [ ! -d "$terraLocalPath" ]; then
    git clone --depth 1 git@github.com:terra-money/LocalTerra.git "$terraLocalPath"
fi
docker-compose --project-directory "$terraLocalPath" rm --force --stop && docker-compose --project-directory "$terraLocalPath" up --detach

cd "$projectPath/scripts" && node --loader ts-node/esm create_astro.ts
cd "$projectPath/scripts" && node --loader ts-node/esm deploy.ts
