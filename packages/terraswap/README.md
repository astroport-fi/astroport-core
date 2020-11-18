# Terraswap: Common Types

This is a collection of common types and the queriers which are commonly used in terraswap contracts.

## Data Types

### AssetInfo

AssetInfo is a convience wrapper to represent the native token and the contract token as a single type.

```rust
#[serde(rename_all = "snake_case")]
pub enum AssetInfo {
    Token { contract_addr: HumanAddr },
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
    pub contract_addr: HumanAddr,
    pub asset_infos: [AssetInfo; 2],
}
```
## Queriers

### Native Token Balance Querier

It uses CosmWasm standard interface to query the account balance to chain.

```rust
pub fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_addr: &HumanAddr,
    denom: String,
) -> StdResult<Uint128>
```

### Token Balance Querier

It provides simliar query interface with [Native-Token-Balance-Querier](Native-Token-Balance-Querier) for CW20 token balance. 

```rust
pub fn query_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    account_addr: &HumanAddr,
) -> StdResult<Uint128>
```

### Token Supply Querier

It provides token supply querier for CW20 token contract.

```rust
pub fn query_supply<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<Uint128>
```

### Pair Info Querier

It also provides the query interface to query avaliable terraswap pair contract info. Any contract can query pair info to terraswap factory contract.

```rust
pub fn query_pair_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<HumanAddr>
```

### Liquidity Token Querier

It returns liquidity token contract address of terraswap pair contract. 

```rust
pub fn query_liquidity_token<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
) -> StdResult<HumanAddr>
```