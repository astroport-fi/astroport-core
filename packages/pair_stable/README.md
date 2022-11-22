# Astroport: StableSwap Pair Interface

This is a collection of types and queriers which are commonly used with Astroport stableswap pair contracts.

---

## InstantiateMsg

Initializes a new stableswap pair.

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

## ExecuteMsg

## ExecuteMsg

### `receive`

Withdraws liquidity or assets that were swapped to (ask assets from a swap operation).

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

Provides liquidity by sending a user's native or token assets to the pool.

__NOTE__: you should increase your token allowance for the pool before providing liquidity!

1. Providing Liquidity Without Specifying Slippage Tolerance

```json
  {
    "provide_liquidity": {
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
      ],
      "auto_stake": false,
      "receiver": "terra..."
    }
  }
```

2. Providing Liquidity With Slippage Tolerance

  ```json
  {
    "provide_liquidity": {
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
      ],
      "slippage_tolerance": "0.01",
      "auto_stake": false,
      "receiver": "terra..."
    }
  }
```

3. Provides the liquidity with a single token. We can do this only for the non-empty pool.

  ```json
  {
    "provide_liquidity": {
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
            "token": {
              "contract_addr": "terra..."
            }
          },
          "amount": "0"
        }
      ],
      "slippage_tolerance": "0",
      "auto_stake": false,
      "receiver": "terra..."
    }
  }
```

### `withdraw_liquidity`

Burn LP tokens and withdraw liquidity from a pool. This call must be sent to a LP token contract associated with the pool from which you want to withdraw liquidity from.

```json
  {
    "withdraw_liquidity": {}
  }
```

### `swap`

Perform a swap. `offer_asset` is your source asset and `to` is the address that will receive the ask assets. All fields are optional except `offer_asset`.

NOTE: You should increase your token allowance for the pool before the swap.

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

Update the pair's configuration.

```json
  {
    "update_config": {
      "params": "<base64_encoded_json_string>: binary serialised parameters for stable pool types; example: {'amp': 100} "
    }
  }
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `pair`

Retrieve a pair's configuration (type, assets traded in it etc).

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

### `query_compute_d`

Returns current D value for the pool.

```json
{
  "query_compute_d": {}
}
```
