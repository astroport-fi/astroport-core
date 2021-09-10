#!/usr/bin/env bash

set -e
set -o pipefail

for c in contracts/*; do
    if [[ "$c" != *"tokenomics" ]]; then
        (cd $c && cargo schema)
    fi
done

for c in contracts/tokenomics/*; do
    (cd $c && cargo schema)
done