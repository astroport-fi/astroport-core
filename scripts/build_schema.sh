#!/usr/bin/env bash

set -e
set -o pipefail

projectPath=$(cd "$(dirname "${0}")" && cd ../ && pwd)

for c in "$projectPath"/contracts/*; do
  if [[ "$c" != *"tokenomics" ]]; then
    if [[ "$c" != *"periphery" ]]; then
      (cd $c && cargo schema)
    fi
  fi
done

for c in "$projectPath"/contracts/tokenomics/*; do
  (cd $c && cargo schema)
done

for c in "$projectPath"/contracts/periphery/*; do
  (cd $c && cargo schema)
done
