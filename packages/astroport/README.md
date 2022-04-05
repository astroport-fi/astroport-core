# Astroport: Common Types

This is a collection of common types and queriers which are commonly used in Astroport contracts.

## Data Types

### AssetInfo

AssetInfo is a convenience wrapper to represent whether a token is the native one (from a specific chain, like LUNA for Terra) or not and it also returns the contract address of that token.

```rust
#[serde(rename_all = "snake_case")]
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

### PairInfo

It is used to represent response data coming from a [Pair-Info-Querier](#Pair-Info-Querier).

```rust
pub struct PairInfo {
    pub contract_addr: Addr,
    pub asset_infos: [AssetInfo; 2],
    pub liquidity_token: Addr,
    pub pair_type: PairType,
}
```

## Queriers

### Native Token Balance Querier

It uses the CosmWasm standard interface to query an account's balance.

```rust
pub fn query_balance(
    deps: &Extern<S, A, Q>,
    account_addr: &Addr,
    denom: String,
) -> StdResult<Uint128>
```

### Token Balance Querier

It provides a similar query interface to [Native-Token-Balance-Querier](Native-Token-Balance-Querier) for fetching CW20 token balances.

```rust
pub fn query_token_balance(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint128>
```

### Token Supply Querier

It fetches a CW20 token's total supply.

```rust
pub fn query_supply(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
) -> StdResult<Uint128>
```

### Pair Info Querier

It returns an Astroport pair contract address if that pair is already available in the factory contract.

```rust
pub fn query_pair_contract(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<Addr>
```

### Liquidity Token Querier

It returns the address of a LP token if that LP token is already available (has a pair) on Astroport.

```rust
pub fn query_liquidity_token(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
) -> StdResult<Addr>
```

## Swap Pairs Simulating

### Simulate

Simulates a swap and returns the output amount, the spread and commission amounts.

```rust
pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse>
```

### Reverse Simulate

Simulates a reverse swap and returns an input amount, the spread and commission amounts.

```rust
pub fn reverse_simulate(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    offer_asset: &Asset,
) -> StdResult<ReverseSimulationResponse>
```
