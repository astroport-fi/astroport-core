#!/usr/bin/env bash

set -e
set -o pipefail

for c in contracts/*; do
    if [[ "$c" != *"tokenomics" ]]; then
        if [[ "$c" != *"periphery" ]]; then
            (cd $c && cargo schema)
        fi
    fi
done

for c in contracts/tokenomics/*; do
    (cd $c && cargo schema)
done

for c in contracts/periphery/*; do
    (cd $c && cargo schema)
done