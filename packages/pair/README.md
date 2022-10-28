# Astroport: Pair Interface

This is a collection of types and queriers which are commonly used with Astroport pair contracts.

## Data Types

### PairInfo

This structure stores the main parameters for an Astroport pair.

```rust
pub struct PairInfo {
    pub asset_infos: [AssetInfo; 2],
    pub contract_addr: Addr,
    pub liquidity_token: Addr,
    pub pair_type: PairType,
}
```

## Queriers

## Swap Pairs Simulating

### Simulate

Simulates a swap and returns the output amount, the spread and commission amounts.

```rust
pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
    offer_asset: &Asset,
    ask_asset_info: Option<AssetInfo>,
) -> StdResult<SimulationResponse>
```

### Reverse Simulate

Simulates a reverse swap and returns an input amount, the spread and commission amounts.

```rust
pub fn reverse_simulate(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
    ask_asset: &Asset,
    offer_asset_info: Option<AssetInfo>,
) -> StdResult<ReverseSimulationResponse>
```
