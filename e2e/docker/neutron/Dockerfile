FROM golang:1.20.0-alpine as builder

RUN apk add --no-cache \
    ca-certificates \
    build-base \
    linux-headers \
    tar
ADD https://github.com/neutron-org/neutron/archive/refs/tags/v2.0.0.tar.gz neutron.tar.gz
RUN mkdir -p neutron && tar -xzf neutron.tar.gz -C neutron --strip-components 1

WORKDIR neutron

RUN --mount=type=cache,target=/root/.cache/go-build \
    --mount=type=cache,target=/root/go/pkg/mod \
    go mod download

# Cosmwasm - Download correct libwasmvm version
RUN WASMVM_VERSION=$(go list -m github.com/CosmWasm/wasmvm | cut -d ' ' -f 2) && \
    wget https://github.com/CosmWasm/wasmvm/releases/download/$WASMVM_VERSION/libwasmvm_muslc.$(uname -m).a \
      -O /lib/libwasmvm_muslc.a && \
    # verify checksum
    wget https://github.com/CosmWasm/wasmvm/releases/download/$WASMVM_VERSION/checksums.txt -O /tmp/checksums.txt && \
    sha256sum /lib/libwasmvm_muslc.a | grep $(cat /tmp/checksums.txt | grep libwasmvm_muslc.$(uname -m) | cut -d ' ' -f 1)

# Build neutrond binary
RUN --mount=type=cache,target=/root/.cache/go-build \
    --mount=type=cache,target=/root/go/pkg/mod \
    go build \
      -mod=readonly \
      -tags "netgo,ledger,muslc,skip_ccv_msg_filter" \
      -ldflags "-X github.com/cosmos/cosmos-sdk/version.Name="neutron" \
              -X github.com/cosmos/cosmos-sdk/version.AppName="neutrond" \
              -X github.com/cosmos/cosmos-sdk/version.BuildTags='netgo,ledger,muslc,skip_ccv_msg_filter' \
              -w -s -linkmode=external -extldflags '-Wl,-z,muldefs -static'" \
      -trimpath \
      -o /neutron/build/neutrond \
      ./cmd/neutrond

FROM alpine:latest

COPY --from=builder /neutron/build/neutrond /bin/neutrond
RUN apk add --no-cache bash jq vim
ADD .neutrond /root/.neutrond

WORKDIR /neutron

CMD ["neutrond", "version"]