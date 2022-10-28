# Astroport: Common Types

This is a collection of common types and queriers which are commonly used in Astroport contracts.

## Data Types

### AssetInfo

AssetInfo is a convenience wrapper to represent whether a token is the native one (from a specific chain, like LUNA for Terra) or not. It also returns the contract address of that token.

```rust
pub enum AssetInfo {
    Token { contract_addr: Addr },
    NativeToken { denom: String },
}
```

### Asset

It contains asset info and a token amount.

```rust
pub struct Asset {
    pub info: AssetInfo,
    pub amount: Uint128,
}
```

## Queriers

### Native Token Balance Querier

It uses the CosmWasm standard interface to query an account's balance.

```rust
pub fn query_balance(
    querier: &QuerierWrapper,
    account_addr: impl Into<String>,
    denom: impl Into<String>,
) -> StdResult<Uint128>
```

### Token Balance Querier

It provides a similar query interface to [Native-Token-Balance-Querier](Native-Token-Balance-Querier) for fetching CW20 token balances.

```rust
pub fn query_token_balance(
    querier: &QuerierWrapper,
    contract_addr: impl Into<String>,
    account_addr: impl Into<String>,
) -> StdResult<Uint128>
```

### Token Supply Querier

It fetches a CW20 token's total supply.

```rust
pub fn query_supply(
    querier: &QuerierWrapper,
    contract_addr: impl Into<String>,
) -> StdResult<Uint128>
```
