# Astroport Pair

The factory may instantiate this contract to create a new x*y=k. It initializes all swap created parameters which can be updated later with owner key.

It creates liquidity token contract as init response, and execute init hook to register created liquidity token contract to self.

README has updated with new messages (Astroport v1 messages follow).

---

### Liquidity Provider

A user can provide liquidity to each pool by sending `provide_liquidity` msg and also can withdraw with `withdraw_liquidity` msg.

Whenever liquidity is deposited into a pool, special tokens known as liquidity tokens are minted to the provider’s address, in proportion to how much liquidity they contributed to the pool. These tokens are a representation of a liquidity provider’s contribution to a pool. Whenever a trade occurs, the `lp_commission%` of fee is distributed pro-rata to all LPs in the pool at the moment of the trade. To receive the underlying liquidity back, plus commission fees that were accrued while their liquidity was locked, LPs must burn their liquidity tokens.

When providing liquidity from a smart contract, the most important thing to keep in mind is that tokens deposited into a pool at any rate other than the current oracle price ratio are vulnerable to being arbitraged. As an example, if the ratio of x:y in a pair is 10:2 (i.e. the price is 5), and someone naively adds liquidity at 5:2 (a price of 2.5), the contract will simply accept all tokens (changing the price to 3.75 and opening up the market to arbitrage), but only issue pool tokens entitling the sender to the amount of assets sent at the proper ratio, in this case 5:1. To avoid donating to arbitrageurs, it is imperative to add liquidity at the current price. Luckily, it’s easy to ensure that this condition is met!

> Note before executing the `provide_liqudity` operation, a user must allow the contract to use the liquidity amount of asset in the token contract.

#### Slippage Tolerance for providing liquidity

If a user specify the slippage tolerance at provide liquidity msg, the contract restricts the operation when the exchange rate is dropped more than the tolerance.

So, at a 1% tolerance level, if a user sends a transaction with 200 UST and 1 ASSET, amountUSTMin should be set to e.g. 198 UST, and amountASSETMin should be set to .99 ASSET. This means that, at worst, liquidity will be added at a rate between 198 ASSET/1 UST and 202.02 UST/1 ASSET (200 UST/.99 ASSET).

#### Slippage tolerance for swap
Astroport has two options to protect traders against slippage during swaps:

1. Providing `max_spread`
The spread is calculated as the difference between the ask amount (using the constant pool price) before and after the swap operation.
Once `max_spread` is set, it will be compared against the actual spread in the swap. In case the spread exceeds the provided max limit, the swap will fail.
Note that the spread is calculated before commission deduction in order to properly represent the pool ratio change.

2. Providing `max_spread` + `belief_price`
If `belief_price` is provided in combination with `max_spread`, the pool will check the difference between the return amount (using `belief_price`) and the real pool price.

Please note that Astroport has the default value for the spread set to 0.5% and the max allowed spread set to 50%.

## InstantiateMsg

Inits a new x*y=k pair.

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
  "init_params": "<base64_encoded_json_string: Optional binary serialised parameters for custom pool types>"
}
```

## ExecuteMsg

### `receive`

Withdrawing provided liquidity or swap assets (only for token contract).

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

Provides pool liquidity by sending user's native or token assets. It can be distinguished with the key under info: token or native_token. NOTE: You should increase token allowance before providing liquidity!

1. Without Slippage Tolerance

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

2. With Slippage Tolerance

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

- Withdraw Liquidity (must be sent to liquidity token contract)

```json
  {
    "withdraw_liquidity": {}
  }
```

### `swap`

Swap between the given two tokens. `offer_asset` is your source asset and `to` is your destination token contract. Fields are optional except `offer_asset`.

NOTE: You should increase token allowance before swap. This method is only used to swap to contract-based token as a destination.

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

Non supported.

```json
  {
    "update_config": {
      "params": "<base64_encoded_json_string>"
    }
  }
```

#### Commission

The `lp_commission` remains in the swap pool. The value of this permanently increased pool goes to all LPs.

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `pair`

Get pair type, assets, etc.

```json
{
  "pair": {}
}
```

### `pool`

Get pool assets and total share.

```json
{
  "pool": {}
}
```

### `config`

Get configuration of pair.

```json
{
  "config": {}
}
```

### `share`

Query share in assets for given amount.

```json
{
  "share": {
    "amount": "123"
  }
}
```

### `simulation`

Simulation swap amounts to get return, spread, commission amounts.

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

Simulation swap to get offer, spread, commission amounts.

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

Query assets last cumulative prices, total share.

```json
{
  "cumulative_prices": {}
}
```
