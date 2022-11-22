# Astroport: ASTRO-xASTRO Pair Interface

This is a collection of types and queriers which are commonly used with Astroport ASTRO-xASTRO pair contract.

---

## InstantiateMsg

Initializes a new ASTRO-xASTRO pair.

```json
{
  "token_code_id": 123,
  "factory_addr": "terra...",
  "asset_infos": [
    {
      "token": {
        "contract_addr": "terra..."
      }
    },
    {
      "token": {
        "contract_addr": "terra..."
      }
    }
  ],
  "init_params": "<base64_encoded_json_string: optional binary serialised parameters for custom pool types>"
}
```

Init params(should be base64 encoded)

```json
{
  "astro_addr": "terra...",
  "xastro_addr": "terra...",
  "staking_addr": "terra..."
}
```

## ExecuteMsg

### `receive`

Allows to swap assets via 3rd party contract. Liquidity providing and withdrawing is not supported.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

### `provide_liquidity`

Liquidity providing is not supported.

### `withdraw_liquidity`

Liquidity withdrawing is not supported.

### `swap`

Perform a swap via Astroport Staking contract.

```json
  {
    "swap": {
      "offer_asset": {
        "info": {
          "native_token": {
            "denom": "uluna"
          }
        },
        "amount": "123"
      },
      "belief_price": "123",
      "max_spread": "123",
      "to": "terra..."
    }
  }
```
### `update_config`

Update config is not supported.

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `pair`

Retrieve a pair's configuration (type, assets traded in it etc)

```json
{
  "pair": {}
}
```

### `pool`

Returns the amount of tokens in the pool for.

```json
{
  "pool": {}
}
```

### `config`

Get the pair contract configuration.

```json
{
  "config": {}
}
```

### `share`

Return the amount of assets someone would get from the pool if they were to burn a specific amount of LP tokens.

```json
{
  "share": {
    "amount": "123"
  }
}
```

### `simulation`

Simulates a swap and returns the spread and commission amounts.

```json
{
  "simulation": {
    "offer_asset": {
      "info": {
        "native_token": {
          "denom": "uusd"
        }
      },
      "amount": "1000000"
    }
  }
}
```

### `reverse_simulation`

Reverse simulates a swap (specifies the ask instead of the offer) and returns the offer amount, spread and commission.

```json
{
  "reverse_simulation": {
    "ask_asset": {
      "info": {
        "token": {
          "contract_addr": "terra..."
        }
      },
      "amount": "1000000"
    }
  }
}
```
### `cumulative_prices`

Returns the cumulative prices for the assets in the pair.

```json
{
  "cumulative_prices": {}
}
```
