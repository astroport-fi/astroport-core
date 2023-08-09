#!/usr/bin/env bash

set -e
set -o pipefail

# terra: https://github.com/terra-money/wasmd/blob/2308975f45eac299bdf246737674482eaa51051c/x/wasm/types/validation.go#L12
# injective: https://github.com/InjectiveLabs/wasmd/blob/e087f275712b5f0a798791495dee0e453d67cad3/x/wasm/types/validation.go#L19
maximum_size=800

for artifact in artifacts/*.wasm; do
  artifactsize=$(du -k "$artifact" | cut -f 1)
  if [ "$artifactsize" -gt $maximum_size ]; then
    echo "Artifact file size exceeded: $artifact"
    echo "Artifact size: $artifactsize"
    echo "Max size: $maximum_size"
    exit 1
  fi
done
