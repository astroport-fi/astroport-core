# Astroport

Uniswap-inspired automated market-maker (AMM) protocol powered by Smart Contracts on the [Terra](https://terra.money) blockchain.

## Contracts

| Name                                                       | Description                                  |
| ---------------------------------------------------------- | -------------------------------------------- |
| [`factory`](contracts/factory)                             |                                              |
| [`pair`](contracts/pair)                                   | Pair with x*y=k curve                        |
| [`pair_stable`](contracts/pair_stable)                     | Pair with stableswap invariant curve         |
| [`token`](contracts/token)                                 | CW20 (ERC20 equivalent) token implementation |
| [`router`](contracts/router)                               | Multi-hop trade router                       |

## Running this contract

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

You can run unit tests on this on each contracts directory via :

```
cargo unit-test
cargo integration-test
```

Once you are happy with the content, you can compile it to wasm on each contracts directory via:

```
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/cw1_subkeys.wasm .
ls -l cw1_subkeys.wasm
sha256sum cw1_subkeys.wasm
```

Or for a production-ready (compressed) build, run the following from the repository root:

```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.10.2
```

The optimized contracts are generated in the artifacts/ directory.
