#!/usr/bin/env bash

set -eu
set -o pipefail

declare CONTRACTS
declare ROOT_DIR
declare FIRST_CRATES
declare SKIP_CRATES
declare DRY_FLAGS

# NOTE: astroport-governance and astro-satellite-package should be published first

if [ -z "${1:-}" ]; then
  echo "Usage: $0 <workspace root dir> [optional: --publish]"
  echo "If flag --publish is not set, only dry-run will be performed."
  echo "NOTE: astroport-governance and astro-satellite-package should be published first."
  exit 1
fi

DRY_FLAGS="--dry-run --allow-dirty"
if [ -z "${2:-}" ]; then
  echo "Dry run mode"
else
  echo "Publishing mode"
  DRY_FLAGS=""
fi

publish() {
  local cargo_error temp_err_file ret_code=0
  local crate="$1"

  echo "Publishing $crate ..."

  set +e

  # Run 'cargo publish' and redirect stderr to a temporary file
  temp_err_file="/tmp/cargo-publish-error-$crate.$$"
  # shellcheck disable=SC2086
  cargo publish -p "$crate" --locked $DRY_FLAGS 2> >(tee "$temp_err_file")
  ret_code=$?
  cargo_error="$(<"$temp_err_file")"
  rm "$temp_err_file"

  set -e

  # Sleep for 60 seconds if the crate was published successfully
  [ $ret_code -eq 0 ] && [ -z "$DRY_FLAGS" ] && sleep 60

  # Check if the error is related to the crate version already being uploaded
  if [[ $cargo_error =~ "the remote server responded with an error" && $cargo_error =~ "is already uploaded" ]]; then
    ret_code=0
  fi

  # Skip if the error is related to the crate version not being found in the registry and
  # the script is running in dry-run mode
  if [[ $cargo_error =~ "no matching package named" || $cargo_error =~ "failed to select a version for the requirement" ]] && \
    [[ -n "$DRY_FLAGS" ]]; then
    ret_code=0
  fi

  # Return the original exit code from 'cargo publish'
  return $ret_code
}

ROOT_DIR="$(realpath "$1")"

FIRST_CRATES="astroport-circular-buffer astroport astroport-factory astroport-pcl-common astro-token-converter"
SKIP_CRATES="astroport-tokenfactory-tracker"

for contract in $FIRST_CRATES; do
  publish "$contract"
done

CONTRACTS="$(cargo metadata --no-deps --locked --manifest-path "$ROOT_DIR/Cargo.toml" --format-version 1 |
  jq -r --arg contracts "$ROOT_DIR/contracts" \
    '.packages[]
    | select(.manifest_path | startswith($contracts))
    | .name')"

echo -e "Publishing crates:\n$CONTRACTS"

for contract in $CONTRACTS; do
  if [[ "$FIRST_CRATES $SKIP_CRATES" == *"$contract"* ]]; then
    continue
  fi

  publish "$contract"
done

echo "ALL DONE"
