# Astroport: Oracle Interface

This is a collection of types and queriers which are commonly used with Astroport oracle contracts.

---

## InstantiateMsg

Initializes the oracle.

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
