# Astroport Pair Bonded Package

Pair bonded package gives a trait that allows implementation pairs with bonded assets(e.g. ASTRO-xASTRO, MARS-xMARS, and other tokens that are correlated but have an increasing exchange rate compared to the other token).
Use [Pair ASTRO-xASTRO](/contracts/pair_astro_xastro/) as example of template implementation.

## InstantiateMsg

Initialize the bonded pair contract.

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

## ExecuteMsg

### `receive`

Allows to swap assets via 3rd party contract. Liquidity providing and withdrawing is not supported in the template.

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

Liquidity providing is not supported in the template by default.

### `withdraw_liquidity`

Liquidity withdrawing is not supported in the template by default.

### `swap`

Swap operation is not implemented in the template by default. You should 

```json
  {
    "swap": {
      "offer_asset": {
        "info": {
          "token": {
            "contract_addr": "terra..."
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

Update config is not supported in the template by default.

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

Simulates a swap (should be implemented in the contract).

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

Reverse simulates a swap (specifies the ask instead of the offer) and returns the offer amount (should be implemented in the contract).

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
