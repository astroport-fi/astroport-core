# Astroport: Factory interface

This is a collection of types and queriers which are commonly used with Astroport factory.

## Data Types

### PairConfig

This structure stores a pair type's configuration.

```rust
pub struct PairConfig {
    /// ID of contract which is allowed to create pairs of this type
    pub code_id: u64,
    /// The pair type (provided in a [`PairType`])
    pub pair_type: PairType,
    /// The total fees (in bps) charged by a pair of this type
    pub total_fee_bps: u16,
    /// The amount of fees (in bps) collected by the Maker contract from this pair type
    pub maker_fee_bps: u16,
    /// Whether a pair type is disabled or not. If it is disabled, new pairs cannot be
    /// created, but existing ones can still read the pair configuration
    pub is_disabled: bool,
    /// Setting this to true means that pairs of this type will not be able
    /// to get an ASTRO generator
    pub is_generator_disabled: bool,
}
```

## Queriers

### Pair Info Querier

Accepts two tokens as input and returns a pair's information.

```rust
pub fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: impl Into<String>,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo>
