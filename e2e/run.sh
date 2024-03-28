#!/usr/bin/env sh

set -e

npm install
npm run bootstrap

while :; do
  if grpcurl --plaintext -connect-timeout 1 localhost:9090 list >/dev/null && grpcurl --plaintext -connect-timeout 1 localhost:39090 list >/dev/null; then
    break
  fi
  echo "Waiting for gRPC services to start..."
done

npm run init
npm run test