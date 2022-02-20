# Astroport Oracle

This demo oracle contract calculates a 1 day TWAP for a xy=k Astroport pool.

---

## InstantiateMsg

Initializes the oracle and checks that the target asset pair type is x*y=k.

```json
{
  "factory_contract": "terra...",
  "asset_infos": [
    {
      "token": {
        "contract_addr": "terra..."
      }
    },
    {
      "native_token": {
        "denom": "uusd"
      }
    }
  ]
}
```

## ExecuteMsg

### `update`

Updates the local TWAP value and the target pair's cumulative prices.

```json
{
  "update": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `consult`

Multiplies a token amount (token that's present in the target pool for the TWAP) by the latest TWAP value for that token.

```json
{
  "consult": {
    "token": {
      "native_token": {
        "denom": "uluna"
      }
    },
    "amount": "1000000"
  }
}
```
