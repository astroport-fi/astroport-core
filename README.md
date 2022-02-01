# Astroport Core

Multi pool type automated market-maker (AMM) protocol powered by smart contracts on the [Terra](https://terra.money) blockchain.

## Contracts diagram

![contract diagram](./assets/sc_diagram.png "Contracts Diagram")

## General Contracts

| Name                                                       | Description                                  |
| ---------------------------------------------------------- | -------------------------------------------- |
| [`factory`](contracts/factory)                             | Pool creation factory                        |
| [`pair`](contracts/pair)                                   | Pair with x*y=k curve                        |
| [`pair_stable`](contracts/pair_stable)                     | Pair with stableswap invariant curve         |
| [`pair_stable_bluna`](contracts/pair_stable_bluna)         | Pair with stableswap invariant curve handling bLUNA rewards for LPs |
| [`token`](contracts/token)                                 | CW20 (ERC20 equivalent) token implementation |
| [`router`](contracts/router)                               | Multi-hop trade router                       |
| [`oracle`](contracts/periphery/oracle)                     | TWAP oracles for x*y=k pool types            |
| [`whitelist`](contracts/whitelist)                         | CW1 whitelist contract                       |

## Tokenomics Contracts

Tokenomics related smart contracts are hosted on ../contracts/tokenomics.

| Name                                                       | Description                                      |
| ---------------------------------------------------------- | ------------------------------------------------ |
| [`generator`](contracts/tokenomics/generator)                                   | Rewards generator for liquidity providers        |
| [`generator_proxy_to_mirror`](contracts/tokenomics/generator_proxy_to_mirror)   | Rewards generator proxy for liquidity providers  |
| [`maker`](contracts/tokenomics/maker)                                           | Fee collector and swapper                        |
| [`staking`](contracts/tokenomics/staking)                                       | xASTRO staking contract                          |
| [`vesting`](contracts/tokenomics/vesting)                                       | ASTRO distributor for generator rewards          |
| [`xASTRO`](contracts/xastro_token)                                              | xASTRO token contract                            |

## Running Contracts from this Repository

You will need Rust 1.44.1+ with wasm32-unknown-unknown target installed.

You can run unit tests for each contract directory via:

```
cargo test
```

You can compile each contract using:

```
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/cw1_subkeys.wasm .
ls -l cw1_subkeys.wasm
sha256sum cw1_subkeys.wasm
```

For a production-ready (compressed) build, run the following from the repository root:

```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.3
```

The optimized contracts are generated in the artifacts/ directory.

## Docs

Docs can be generated using `cargo doc --no-deps`

## Bug Bounty

The contracts in this repo are included in a [bug bounty program](https://www.immunefi.com/bounty/astroport).
