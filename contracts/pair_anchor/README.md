# Astroport Anchor Product Pair

This pool can be used for swapping UST-aUST using Anchor mint and burn mechanism.

---

## Liquidity Providers

You cannot provide liquidity to the anchor pair, as it uses burn / mint mechanism of anchor for swaps.

### Slippage Tolerance for Providing Liquidity

You cannot provide liquidity and there is no slippage.

## InstantiateMsg

Initializes a new x*y=k pair.

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
      "native_token": {
        "denom": "uusd"
      }
    }
  ],
  "init_params": "<base64_encoded_json_string: optional binary serialised parameters for custom pool types>"
}
```

For the custom pool type see:

```json
"terra...",
```

## ExecuteMsg

### `receive`

Withdraws liquidity or assets that were swapped to (ask assets in a swap operation).

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

Not supported

### `withdraw_liquidity`

Not supported

### `swap`

Perform a swap. `offer_asset` is your source asset and `to` is the address that will receive the ask assets. All fields are optional except `offer_asset`.

NOTE: You should increase token allowance before swap.

```json
  {
    "swap": {
      "offer_asset": {
        "info": {
          "native_token": {
            "denom": "uusd"
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

Not supported

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

Returns the amount of tokens in the pool for all assets as well as the amount of LP tokens issued.

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

Returns always an empty array.

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
For this pair it is always empty.

```json
{
  "cumulative_prices": {}
}
```
