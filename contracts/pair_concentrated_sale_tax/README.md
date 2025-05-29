# Astroport Concentrated Liquidity Pair

[//]: # (TODO: write README)

## InstantiateMsg

Initializes a new concentrated liquidity pair.

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
  "init_params": "<base64_encoded_json_string>"
}
```

where `<base64_encoded_json_string>` is

```json
{
  "amp": "40.0",
  "gamma": "0.0001",
  "mid_fee": "0.005",
  "out_fee": "0.01",
  "fee_gamma": "0.001",
  "repeg_profit_threshold": "0.0001",
  "min_price_scale_delta": "0.000001",
  "initial_price_scale": "1.5",
  "ma_half_time": 600,
  "owner": "terra..."
}
```

Note, the aforementioned values are just examples and have no practical meaning.

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
    "receiver": "terra...",
    "slippage_tolerance": "0.01"
  }
}
```

### `withdraw_liquidity`

Burn LP tokens and withdraw liquidity from a pool. This call must be sent to a LP token contract associated with the
pool from which you want to withdraw liquidity from.

```json
{
  "withdraw_liquidity": {}
}
```

### `swap`

Perform a swap. `offer_asset` is your source asset and `to` is the address that will receive the ask assets. All fields
are optional except `offer_asset`.

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

Update the concentrated liquidity pair's configuration.

```json
{
  "update_config": {
    "params": "<base64_encoded_json_string>"
  }
}
```

where `<base64_encoded_json_string>` is one of

1. Update parameters

```json
{
  "update": {
    "mid_fee": "0.1",
    "out_fee": "0.01",
    ...
  }
}
```

2. Update Amp or Gamma

```json
{
  "promote": {
    "next_amp": "44",
    "next_gamma": "0.001",
    "future_time": 1570257049
  }
}
```

3. Stop Amp and Gamma change

```json
{
  "stop_changing_amp_gamma": {}
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

### `lp_price`

Query LP token virtual price.

```json
{
  "lp_price": {}
}
```

### `amp_gamma`

Query curremt Amp and Gamma parameters.

```json
{
  "amp_gamma": {}
}
```

### `asset_balance_at`

Returns the balance of the specified asset that was in the pool just preceeding the moment of the specified block height creation. It will return None (null) if the balance was not tracked up to the specified block height.

```json
{
  "asset_balance_at": {
    "asset_info": {
      "native_token": {
        "denom": "stake"
      }
    },
    "block_height": "12345678"
  }
}
```

`observe`

Query price from stored observations. If observation was not found at exact time then it is interpolated using surrounding observations.

```json
{
  "observe": {
    "seconds_ago": 3600
  }
}
```
