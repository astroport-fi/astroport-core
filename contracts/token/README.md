# Astroport Token

README has updated with new messages (Astroport v1 messages follow).

---
# CW20 based token contract

This is a basic implementation of a cw20-base contract [CW20-base](https://github.com/CosmWasm/cw-plus/tree/main/contracts/cw20-base). It implements the [CW20 spec](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw20) and is designed to be imported into other contracts to easily build cw20-compatible tokens with custom logic.

Astroport contracts logic based on native and token assets. So, they use this contract for creating tokens (LP, ASTRO, etc.).

## Importing this contract

You can also import much of the logic of this contract to build another contract to extend what you need. Basically, you just need to write your instantiate, execute, query entrypoints and import `cw20_execute`, `cw20_query`. You _could_ reuse `instantiate` as it, but it is likely you will want to change it. And it is rather simple.