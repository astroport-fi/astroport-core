# Astroport Oracle

The oracle contract performs calculation x*y=k pair assets average prices based on accumulations and time period (day).

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with factory contract to check asset pair type is x*y=k.

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

Updates pair average and cumulative prices.

```json
{
  "update": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `consult`

Multiplies a given amount and last average price in common.

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
