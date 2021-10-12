# Astroport: Common Types

This is a collection of common types and the queriers which are commonly used in astroport contracts.

## Data Types

### AssetInfo

AssetInfo is a convenience wrapper to represent the native token and the contract token as a single type.

```rust
#[serde(rename_all = "snake_case")]
pub enum AssetInfo {
    Token { contract_addr: Addr },
    NativeToken { denom: String },
}
```

### Asset

It contains asset info with the amount of token.

```rust
pub struct Asset {
    pub info: AssetInfo,
    pub amount: Uint128,
}
```

### PairInfo

It is used to represent response data of [Pair-Info-Querier](#Pair-Info-Querier)

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

It uses CosmWasm standard interface to query the account balance to chain.

```rust
pub fn query_balance(
    deps: &Extern<S, A, Q>,
    account_addr: &Addr,
    denom: String,
) -> StdResult<Uint128>
```

### Token Balance Querier

It provides similar query interface with [Native-Token-Balance-Querier](Native-Token-Balance-Querier) for CW20 token balance.

```rust
pub fn query_token_balance(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint128>
```

### Token Supply Querier

It provides token supply querier for CW20 token contract.

```rust
pub fn query_supply(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
) -> StdResult<Uint128>
```

### Pair Info Querier

It also provides the query interface to query available astroport pair contract info. Any contract can query pair info to astroport factory contract.

```rust
pub fn query_pair_contract(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<Addr>
```

### Liquidity Token Querier

It returns liquidity token contract address of astroport pair contract.

```rust
pub fn query_liquidity_token(
    deps: &Extern<S, A, Q>,
    contract_addr: &Addr,
) -> StdResult<Addr>
```

## Swap Pairs Simulating

### Simulate

Returns simulation swap return, spread, commission amounts.

```rust
pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse>
```

### Reverse Simulate

Returns simulation swap offer, spread, commission amounts.

```rust
pub fn reverse_simulate(
    querier: &QuerierWrapper,
    pair_contract: Addr,
    offer_asset: &Asset,
) -> StdResult<ReverseSimulationResponse>
```