#!/usr/bin/env sh

while :; do
  echo "Waiting for nodes to start..."
  if curl -s neutron:26657 >/dev/null && curl -s terra:26657 >/dev/null; then
    break
  fi
  sleep 1
done

hermes start