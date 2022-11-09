# Astroport Core

[![codecov](https://codecov.io/gh/astroport-fi/astroport-core/branch/release/graph/badge.svg?token=ROOLZTGZMM)](https://codecov.io/gh/astroport-fi/astroport-core)

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
| [`xastro_token`](contracts/tokenomics/xastro_token)                             | xASTRO token contract                            |

## Building Contracts

You will need Rust 1.64.0+ with wasm32-unknown-unknown target installed.

### You can compile each contract:
Go to contract directory and run 
    
```
cargo wasm
cp ../../target/wasm32-unknown-unknown/release/astroport_token.wasm .
ls -l astroport_token.wasm
sha256sum astroport_token.wasm
```

### You can run tests for all contracts
Run the following from the repository root

```
cargo test
```

### For a production-ready (compressed) build:
Run the following from the repository root

```
./scripts/build_release.sh
```

The optimized contracts are generated in the artifacts/ directory.

## Branches

We use [main](https://github.com/astroport-fi/astroport-core/tree/main) branch for new feature development and [release](https://github.com/astroport-fi/astroport-core/tree/release) one for collecting features which are ready for deployment. You can find the version and commit for actually deployed contracts [here](https://github.com/astroport-fi/astroport-changelog).

## Docs

Docs can be generated using `cargo doc --no-deps`

## Bug Bounty

The contracts in this repo are included in a [bug bounty program](https://www.immunefi.com/bounty/astroport).
