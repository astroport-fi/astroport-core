# Astroport Maker

The maker contract collects pair assets per pool (following to factory's `maker_fee`) tries to swap it to ASTRO and sends to staking and governance (following to `governance_percent`).

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with required contract addresses and `governance_percent`.

```json
{
  "astro_token_contract": "terra...",
  "factory_contract": "terra...",
  "staking_contract": "terra...",
  "governance_contract": "terra...",
  "governance_percent": "20"
}
```

## ExecuteMsg

### `collect`

Collects assets from given pools.

```json
{
  "collect": {
    "pair_addresses": [
      "terra..."
    ]
  }
}
```

### `set_config`

Updates config, all fields are optional.

```json
{
  "staking_contract": "terra...",
  "governance_contract": "terra...",
  "governance_percent": "20"
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns contract addresses and `governance_percent`.

```json
{
  "config": {}
}
```

### `balances`

Returns asset infos for all pools.

```json
{
  "balances": {
    "assets": [
      {
        "info": {
          "token": {
            "contract_addr": "terra..."
          }
        },
        "amount": "1000000"
      },
      {
        "info": {
          "native_token": {
            "denom": "uusd"
          }
        },
        "amount": "1000000"
      }
    ]
  }
}
```
