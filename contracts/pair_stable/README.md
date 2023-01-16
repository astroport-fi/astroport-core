# Astroport Base Stableswap Pair

The stableswap pool uses the 4A(Rx+Ry) + D formula, resulting in a constant price ∆x / ∆y = 1. More details around how the pool functions can be found [here](https://docs.astroport.fi/astroport/astroport/astro-pools/stableswap-invariant-pools).

---

### Liquidity Providers

A user can provide liquidity to a constant product pool by calling `provide_liquidity`. Users can also withdraw liquidity by calling `withdraw_liquidity`.

Whenever liquidity is deposited into a pool, special tokens known as "liquidity tokens" are minted to the provider’s address, in proportion to how much liquidity they contributed to the pool. These tokens are a representation of a liquidity provider’s contribution to a pool. Whenever a trade occurs, the `lp_commission` is distributed pro-rata to all LPs in the pool at the moment of the trade. To receive the underlying liquidity back plus accrued LP fees, LPs must burn their liquidity tokens.

When providing liquidity from a smart contract, the most important thing to keep in mind is that the amount of tokens deposited into a pool and the amount of tokens withdrawn later from the pool will most likely not be the same (even if stableswap encourages a constant 1:1 ratio between all assets in the pool).

As an example, let's say the global ratio between two tokens x:y is 1.01:1 (1 x = 0.99 y), but the current ratio between the tokens in an Astroport pair is 1:1.01 (1 x = 1.01 y). Let's also say that someone may decide to LP in the x:y Astroport pool at the current 1:1.01 ratio. As the Astroport pool gets arbitraged to the global ratio, the amount of x & y tokens that the LP can withdraw changes because the total amounts of x & y tokens in the pool also change.

> Note that before executing the `provide_liqudity` operation, a user must allow the pool contract to take tokens from their wallet

### Slippage Tolerance for Providing Liquidity

If a user specifies a slippage tolerance when they provide liquidity in a constant product pool, the pool contract makes sure that the transaction goes through only if the pool price does not change more than tolerance.

As an example, let's say someone LPs in a pool and specifies a 1% slippage tolerance. The user LPs 200 UST and 200 `ASSET`. With a 1% slippage tolerance, `amountUSTMin` (the minimum amount of UST to LP) should be set to 198 UST, and `amountASSETMin` (the minimum amount of `ASSET` to LP) should be set to .99 `ASSET`. This means that, in a worst case scenario, liquidity will be added at a pool rate of 198 `ASSET`/1 UST or 202.02 UST/1 `ASSET` (200 UST + .99 `ASSET`). If the contract cannot add liquidity within these bounds (because the pool ratio changed more than the tolerance), the transaction will revert.

## Traders

### Slippage Tolerance for Swaps

Astroport has two options to protect traders against slippage during swaps:

1. Providing `max_spread`
The spread is calculated as the difference between the ask amount (using the constant pool price) before and after the swap operation. Once `max_spread` is set, it will be compared against the actual swap spread. In case the swap spread exceeds the provided max limit, the swap will fail.

Note that the spread is calculated before commission deduction in order to properly represent the pool's ratio change.

2. Providing `max_spread` + `belief_price`
If `belief_price` is provided in combination with `max_spread`, the pool will check the difference between the return amount (using `belief_price`) and the real pool price.

Please note that Astroport has the default value for the spread set to 0.5% and the max allowed spread set to 50%.

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

2. Provides the liquidity with a single token. We can do this only for the non-empty pool.

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
