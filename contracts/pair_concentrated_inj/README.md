# Astroport Concentrated Liquidity Pair with Injective Orderbook integration

[//]: # (TODO: write README)

## Limitations
1. This implementation is intended for Injective chain only. You won't be able to store this contract anywhere else.
2. Asset infos order is important while creating a new CL pool!
   If you want to create CL pool and integrate with orderbook you must specify base_asset as a first asset and quote_asset as a second asset.
   More info in [Injective docs](https://docs.injective.network/develop/modules/Injective/exchange/spot_market_concepts#definitions).
3. This pair contract does not support CW20 tokens. Only native tokens are allowed.
4. When registering this contract in begin blocker, the proposer must ask to pin contract in memory to make orderbook integration profitable.

## InstantiateMsg

Initializes a new concentrated liquidity pair.

```json
{
  "token_code_id": 123,
  "factory_addr": "inj...",
  "asset_infos": [
    {
      "native_token": {
        "denom": "inj"
      }
    },
    {
      "native_token": {
        "denom": "peggy..."
      }
    }
  ],
  "init_params": "<base64_encoded_json_string>"
}
```

where `<base64_encoded_json_string>` is

```json
{
  "main_params": {
    "amp": "40.0",
    "gamma": "0.0001",
    "mid_fee": "0.005",
    "out_fee": "0.01",
    "fee_gamma": "0.001",
    "repeg_profit_threshold": "0.0001",
    "min_price_scale_delta": "0.000001",
    "initial_price_scale": "1.5",
    "ma_half_time": 600,
    "owner": "inj..."
  },
  "orderbook_config": {
    "market_id": "0x...",
    "orders_number": "5",
    "min_trades_to_avg": "500"
  }
}
```

Note, the aforementioned values are just examples and have no practical meaning.

## ExecuteMsg

### `receive`

Withdraws liquidity by providing LP token.

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

Provides liquidity by sending a user's assets to the pool.

```json
{
  "provide_liquidity": {
    "assets": [
      {
        "info": {
          "native_token": {
            "denom": "inj"
          }
        },
        "amount": "1000000"
      },
      {
        "info": {
          "native_token": {
            "denom": "peggy..."
          }
        },
        "amount": "1000000"
      }
    ],
    "auto_stake": false,
    "receiver": "inj...",
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
          "denom": "inj"
        }
      },
      "amount": "123"
    },
    "belief_price": "123",
    "max_spread": "123",
    "to": "inj..."
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

4. Update orderbook params

```json
{
  "update_orderbook_params": {
    "orders_number": 3
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
          "denom": "inj"
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
        "native_token": {
          "denom": "peggy..."
        }
      },
      "amount": "1000000"
    }
  }
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

Query current Amp and Gamma parameters.

```json
{
  "amp_gamma": {}
}
```

### `observe`

Query price from stored observations. If observation was not found at exact time then it is interpolated using surrounding observations.

```json
{
  "observe": {
    "seconds_ago": 3600
  }
}
```

### `orderbook_state`

Query current orderbook integration params and state.

```json
{
  "orderbook_state": {}
}
```